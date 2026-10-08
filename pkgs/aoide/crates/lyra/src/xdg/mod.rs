//! The workspace's only freedesktop key-file reader.
//!
//! Three file formats share one grammar, and this module reads all three:
//! `.desktop` files ([`entry`]), icon-theme `index.theme` files
//! ([`icon_theme`]) and `gtk-3.0/settings.ini` (the user's icon theme name,
//! read by [`icon_theme::Resolver`]). The grammar itself — groups, `key=value`,
//! `key[locale]=value`, the value types — is [`keyfile`].
//!
//! Two consumers read through it: `lyra apps` (`commands::apps`, the listing
//! and `song/stage/apps.json`) and the planned `lyra launch`. Both take a [`Env`], never
//! the process environment, so a test builds an `Env` over temp dirs. A
//! second parser for any of the three formats does not belong anywhere in the
//! workspace: widen this module instead (`pkgs/aoide/crates/AGENTS.md`, "no
//! cross-crate copying").

pub mod entry;
pub mod icon_theme;
pub mod keyfile;

use std::path::PathBuf;

/// Where desktop entries and icon themes are looked up, and for whom.
#[derive(Debug, Clone)]
pub struct Env {
    /// `$XDG_DATA_HOME` then `$XDG_DATA_DIRS`, in precedence order.
    pub data_dirs: Vec<PathBuf>,
    pub config_home: PathBuf,
    pub home: PathBuf,
    /// `$XDG_CURRENT_DESKTOP` split on `:`; empty when unset.
    pub desktops: Vec<String>,
    pub locale: Option<Locale>,
}

impl Env {
    /// The base-directory spec's reading of the process environment: an empty
    /// or relative path variable counts as unset.
    pub fn from_process() -> Env {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let absolute = |k: &str| var(k).map(PathBuf::from).filter(|p| p.is_absolute());
        let home = absolute("HOME").unwrap_or_else(aoide_protocol::aoide_home);
        let data_home = absolute("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        let system = var("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        let data_dirs = std::iter::once(data_home)
            .chain(system.split(':').map(PathBuf::from).filter(|p| p.is_absolute()))
            .collect();
        Env {
            data_dirs,
            config_home: absolute("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")),
            desktops: var("XDG_CURRENT_DESKTOP")
                .map(|v| v.split(':').filter(|d| !d.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
            locale: ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| var(k)).and_then(|v| Locale::parse(&v)),
            home,
        }
    }
}

/// `lang_COUNTRY@MODIFIER`, the parts a localized key can match on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locale {
    lang: String,
    country: Option<String>,
    modifier: Option<String>,
}

impl Locale {
    /// Reads `lang_COUNTRY.ENCODING@MODIFIER` and drops the encoding. `C`,
    /// `POSIX` and the empty string name no language.
    pub fn parse(spec: &str) -> Option<Locale> {
        let (rest, modifier) = match spec.split_once('@') {
            Some((rest, modifier)) => (rest, Some(modifier.to_string())),
            None => (spec, None),
        };
        let rest = rest.split('.').next().unwrap_or_default();
        let (lang, country) = match rest.split_once('_') {
            Some((lang, country)) => (lang, Some(country.to_string())),
            None => (rest, None),
        };
        if matches!(lang, "" | "C" | "POSIX") {
            return None;
        }
        Some(Locale { lang: lang.to_string(), country, modifier })
    }

    /// The localized-key suffixes to try, best first: `lang_COUNTRY@MODIFIER`,
    /// `lang_COUNTRY`, `lang@MODIFIER`, `lang`.
    pub fn keys(&self) -> Vec<String> {
        let (lang, country, modifier) = (&self.lang, self.country.as_deref(), self.modifier.as_deref());
        let mut keys = Vec::new();
        if let (Some(c), Some(m)) = (country, modifier) {
            keys.push(format!("{lang}_{c}@{m}"));
        }
        if let Some(c) = country {
            keys.push(format!("{lang}_{c}"));
        }
        if let Some(m) = modifier {
            keys.push(format!("{lang}@{m}"));
        }
        keys.push(lang.clone());
        keys
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use super::Env;
    use std::path::PathBuf;

    /// A temp tree that removes itself; `Env`s over it never see the real XDG dirs.
    pub struct Scratch(pub PathBuf);

    impl Scratch {
        pub fn new(tag: &str) -> Scratch {
            Scratch(aoide_test_support::unique_tmp(tag))
        }

        pub fn path(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }

        pub fn write(&self, rel: &str, text: &str) -> PathBuf {
            let path = self.path(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }

        pub fn touch(&self, rel: &str) -> PathBuf {
            self.write(rel, "")
        }

        /// Data dirs are given relative to the scratch root; the desktop is `Hyprland`.
        pub fn env(&self, data_dirs: &[&str]) -> Env {
            Env {
                data_dirs: data_dirs.iter().map(|d| self.path(d)).collect(),
                config_home: self.path("config"),
                home: self.path("home"),
                desktops: vec!["Hyprland".into()],
                locale: None,
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub fn desktop(body: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName=X\nExec=x\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, EnvSaver};

    #[test]
    fn locale_drops_the_encoding_and_orders_its_keys() {
        let l = Locale::parse("de_DE.UTF-8@euro").unwrap();
        assert_eq!(l.keys(), ["de_DE@euro", "de_DE", "de@euro", "de"]);
        assert_eq!(Locale::parse("en_US.UTF-8").unwrap().keys(), ["en_US", "en"]);
        assert_eq!(Locale::parse("sr@latin").unwrap().keys(), ["sr@latin", "sr"]);
        assert_eq!(Locale::parse("fr").unwrap().keys(), ["fr"]);
    }

    #[test]
    fn c_posix_and_empty_name_no_language() {
        for spec in ["C", "POSIX", "C.UTF-8", ""] {
            assert_eq!(Locale::parse(spec), None, "{spec}");
        }
    }

    #[test]
    fn from_process_reads_the_base_dir_variables() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let keys = [
            "HOME", "XDG_DATA_HOME", "XDG_DATA_DIRS", "XDG_CONFIG_HOME", "XDG_CURRENT_DESKTOP", "LC_ALL", "LC_MESSAGES", "LANG",
        ];
        let _saved = EnvSaver::capture(&keys);

        std::env::set_var("HOME", "/h");
        std::env::set_var("XDG_DATA_HOME", "");
        std::env::set_var("XDG_DATA_DIRS", "/a:rel/b::/c");
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CURRENT_DESKTOP", "Hyprland:GNOME");
        std::env::remove_var("LC_ALL");
        std::env::remove_var("LC_MESSAGES");
        std::env::set_var("LANG", "de_DE.UTF-8");
        let env = Env::from_process();
        let dirs: Vec<_> = env.data_dirs.iter().map(|d| d.to_str().unwrap()).collect();
        assert_eq!(dirs, ["/h/.local/share", "/a", "/c"], "empty and relative values are ignored");
        assert_eq!(env.config_home, PathBuf::from("/h/.config"));
        assert_eq!(env.desktops, ["Hyprland", "GNOME"]);
        assert_eq!(env.locale, Locale::parse("de_DE"));

        std::env::remove_var("XDG_DATA_DIRS");
        std::env::remove_var("XDG_CURRENT_DESKTOP");
        std::env::set_var("XDG_DATA_HOME", "/dh");
        std::env::set_var("XDG_CONFIG_HOME", "/ch");
        std::env::set_var("LC_ALL", "C");
        let env = Env::from_process();
        let dirs: Vec<_> = env.data_dirs.iter().map(|d| d.to_str().unwrap()).collect();
        assert_eq!(dirs, ["/dh", "/usr/local/share", "/usr/share"]);
        assert_eq!(env.config_home, PathBuf::from("/ch"));
        assert!(env.desktops.is_empty(), "an unset desktop is the empty list");
        assert_eq!(env.locale, None, "LC_ALL=C is the first set variable and names no language");
    }
}
