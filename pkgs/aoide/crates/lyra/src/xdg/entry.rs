//! Desktop entries: finding the `.desktop` file an id names, reading it, and
//! deciding whether a launcher lists it.
//!
//! An id is the file's path under `applications/` with `/` turned into `-` and
//! `.desktop` dropped (`kde4/foo.desktop` is `kde4-foo`). The first data dir
//! holding an id wins and a later dir never resurrects it: the winning file
//! alone decides, whether it reads, parses, is `Hidden` or is merely
//! unlisted. Symlinks are followed, so a profile or flatpak export that is a
//! link tree reads like a real one.

use super::keyfile::KeyFile;
use super::Env;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const GROUP: &str = "Desktop Entry";
const MAX_BYTES: u64 = 1 << 20;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub file: PathBuf,
    /// `Type`; only `Application` is launchable.
    pub kind: Option<String>,
    pub name: Option<String>,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    /// The `Icon` value as written: a theme name or an absolute path.
    pub icon: Option<String>,
    /// `Exec` after the key-file string unescape and nothing more. The field
    /// codes and the argument quoting rules apply to exactly this value, and
    /// `lyra launch` is the only reader that applies them.
    pub exec: Option<String>,
    pub path: Option<String>,
    pub terminal: bool,
    pub hidden: bool,
    pub no_display: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub try_exec: Option<String>,
    pub dbus_activatable: bool,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub startup_wm_class: Option<String>,
    /// In `Actions=` order; an action without a `[Desktop Action <id>]` group or a `Name` is absent.
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub exec: Option<String>,
}

/// Why a launcher leaves a readable, non-`Hidden` entry out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    NotApplication,
    NoName,
    NoExec,
    NoDisplay,
    OnlyShowIn,
    NotShowIn,
    TryExec,
}

impl Filter {
    pub fn as_str(self) -> &'static str {
        match self {
            Filter::NotApplication => "not-application",
            Filter::NoName => "no-name",
            Filter::NoExec => "no-exec",
            Filter::NoDisplay => "no-display",
            Filter::OnlyShowIn => "only-show-in",
            Filter::NotShowIn => "not-show-in",
            Filter::TryExec => "try-exec",
        }
    }
}

/// What the winning file for an id turned out to be.
#[derive(Debug)]
pub enum Found {
    Entry(Box<Entry>),
    Invalid { id: String, file: PathBuf, reason: String },
}

impl Found {
    pub fn id(&self) -> &str {
        match self {
            Found::Entry(e) => &e.id,
            Found::Invalid { id, .. } => id,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Missing {
    Unknown,
    /// The winning file says `Hidden=true`, which deletes the id.
    Hidden,
    Invalid(String),
}

impl Entry {
    /// The reason a launcher omits this entry, `None` when it lists it.
    pub fn unlisted(&self, env: &Env) -> Option<Filter> {
        let shown_in = |names: &[String]| names.iter().any(|n| env.desktops.contains(n));
        if self.kind.as_deref() != Some("Application") {
            return Some(Filter::NotApplication);
        }
        if self.name.is_none() {
            return Some(Filter::NoName);
        }
        if self.exec.is_none() {
            return Some(Filter::NoExec);
        }
        if self.no_display {
            return Some(Filter::NoDisplay);
        }
        if !self.only_show_in.is_empty() && !shown_in(&self.only_show_in) {
            return Some(Filter::OnlyShowIn);
        }
        if shown_in(&self.not_show_in) {
            return Some(Filter::NotShowIn);
        }
        if self.try_exec.as_deref().is_some_and(|t| !try_exec_runs(t)) {
            return Some(Filter::TryExec);
        }
        None
    }
}

// A relative TryExec is never looked up: a user unit's PATH is not the session's, and the spec makes the check optional.
fn try_exec_runs(target: &str) -> bool {
    let path = Path::new(target);
    !path.is_absolute() || std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Every id once, each read from its winning file, in data-dir order.
pub fn scan(env: &Env) -> Vec<Found> {
    winners(env).into_iter().map(|(id, file)| load(env, id, file)).collect()
}

pub fn find(env: &Env, id: &str) -> Result<Entry, Missing> {
    let (id, file) = winners(env).into_iter().find(|(i, _)| i == id).ok_or(Missing::Unknown)?;
    match load(env, id, file) {
        Found::Entry(e) if e.hidden => Err(Missing::Hidden),
        Found::Entry(e) => Ok(*e),
        Found::Invalid { reason, .. } => Err(Missing::Invalid(reason)),
    }
}

/// The ids [`find`] answers with an entry.
pub fn ids(env: &Env) -> Vec<String> {
    scan(env)
        .into_iter()
        .filter_map(|f| match f {
            Found::Entry(e) if !e.hidden => Some(e.id),
            _ => None,
        })
        .collect()
}

/// Every `*.desktop` under `applications/` as (path relative to it, path), symlinks followed, sorted.
pub fn desktop_files(applications: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    walk(applications, "", &mut Vec::new(), &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, rel: &str, ancestors: &mut Vec<PathBuf>, out: &mut Vec<(String, PathBuf)>) {
    let Ok(canonical) = std::fs::canonicalize(dir) else { return };
    if ancestors.contains(&canonical) {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else { return };
    ancestors.push(canonical);
    for item in read.filter_map(Result::ok) {
        let Some(name) = item.file_name().to_str().map(String::from) else { continue };
        let path = item.path();
        if path.is_dir() {
            walk(&path, &format!("{rel}{name}/"), ancestors, out);
        } else if name.ends_with(".desktop") {
            out.push((format!("{rel}{name}"), path));
        }
    }
    ancestors.pop();
}

fn winners(env: &Env) -> Vec<(String, PathBuf)> {
    let mut taken = HashSet::new();
    let mut out = Vec::new();
    for dir in &env.data_dirs {
        for (rel, file) in desktop_files(&dir.join("applications")) {
            let id = rel.strip_suffix(".desktop").unwrap_or(&rel).replace('/', "-");
            if taken.insert(id.clone()) {
                out.push((id, file));
            }
        }
    }
    out
}

fn load(env: &Env, id: String, file: PathBuf) -> Found {
    match read(&file).and_then(|text| parse(env, &id, &file, &text)) {
        Ok(entry) => Found::Entry(Box::new(entry)),
        Err(reason) => Found::Invalid { id, file, reason },
    }
}

fn read(file: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(file).map_err(|e| format!("cannot read it: {}", e.kind()))?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if meta.len() > MAX_BYTES {
        return Err("larger than 1 MiB".into());
    }
    let bytes = std::fs::read(file).map_err(|e| format!("cannot read it: {}", e.kind()))?;
    String::from_utf8(bytes).map_err(|_| "not UTF-8".into())
}

fn parse(env: &Env, id: &str, file: &Path, text: &str) -> Result<Entry, String> {
    let kf = KeyFile::parse(text).map_err(|e| e.to_string())?;
    if !kf.has_group(GROUP) {
        return Err("no [Desktop Entry] group".into());
    }
    let locale = env.locale.as_ref();
    let nonempty = |v: String| Some(v).filter(|v| !v.is_empty());
    let string = |group: &str, key: &str| kf.string(group, key).and_then(nonempty);
    let localized = |group: &str, key: &str| kf.localestring(group, key, locale).and_then(nonempty);
    let list = |key: &str| kf.list(GROUP, key).unwrap_or_default();
    let flag = |key: &str| kf.boolean(GROUP, key).unwrap_or(false);
    let actions = list("Actions")
        .into_iter()
        .filter_map(|action| {
            let group = format!("Desktop Action {action}");
            Some(Action {
                name: localized(&group, "Name")?,
                icon: string(&group, "Icon"),
                exec: string(&group, "Exec"),
                id: action,
            })
        })
        .collect();
    Ok(Entry {
        id: id.to_string(),
        file: file.to_path_buf(),
        kind: string(GROUP, "Type"),
        name: localized(GROUP, "Name"),
        generic_name: localized(GROUP, "GenericName"),
        comment: localized(GROUP, "Comment"),
        icon: string(GROUP, "Icon"),
        exec: string(GROUP, "Exec"),
        path: string(GROUP, "Path"),
        terminal: flag("Terminal"),
        hidden: flag("Hidden"),
        no_display: flag("NoDisplay"),
        only_show_in: list("OnlyShowIn"),
        not_show_in: list("NotShowIn"),
        try_exec: string(GROUP, "TryExec"),
        dbus_activatable: flag("DBusActivatable"),
        keywords: kf.localelist(GROUP, "Keywords", locale).unwrap_or_default(),
        categories: list("Categories"),
        startup_wm_class: string(GROUP, "StartupWMClass"),
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xdg::fixture::{desktop, Scratch};
    use crate::xdg::Locale;

    fn entry_in(s: &Scratch, env: &Env, text: &str) -> Entry {
        s.write("share/applications/app.desktop", text);
        find(env, "app").unwrap()
    }

    #[test]
    fn a_nested_file_takes_the_dash_joined_id() {
        let s = Scratch::new("xdg-entry-id");
        s.write("share/applications/kde4/foo.desktop", &desktop(""));
        s.write("share/applications/plain.desktop", &desktop(""));
        s.write("share/applications/readme.txt", "not an entry");
        let env = s.env(&["share"]);
        let mut ids = ids(&env);
        ids.sort();
        assert_eq!(ids, ["kde4-foo", "plain"]);
        assert_eq!(find(&env, "kde4-foo").unwrap().file, s.path("share/applications/kde4/foo.desktop"));
    }

    #[test]
    fn the_first_data_dir_holding_an_id_wins() {
        let s = Scratch::new("xdg-entry-first");
        s.write("user/applications/a.desktop", "[Desktop Entry]\nType=Application\nName=User\nExec=u\n");
        s.write("system/applications/a.desktop", "[Desktop Entry]\nType=Application\nName=System\nExec=s\n");
        s.write("system/applications/b.desktop", &desktop(""));
        let env = s.env(&["user", "system"]);
        assert_eq!(find(&env, "a").unwrap().name.as_deref(), Some("User"));
        assert_eq!(scan(&env).len(), 2, "a shadowed file is not a second entry");
        let swapped = s.env(&["system", "user"]);
        assert_eq!(find(&swapped, "a").unwrap().name.as_deref(), Some("System"));
    }

    #[test]
    fn a_hidden_override_deletes_the_system_id() {
        let s = Scratch::new("xdg-entry-hidden");
        s.write("user/applications/a.desktop", &desktop("Hidden=true\n"));
        s.write("system/applications/a.desktop", &desktop(""));
        let env = s.env(&["user", "system"]);
        assert_eq!(find(&env, "a").unwrap_err(), Missing::Hidden);
        assert!(ids(&env).is_empty(), "a hidden id is not offered as a suggestion either");
    }

    #[test]
    fn an_invalid_winner_keeps_the_id_taken() {
        let s = Scratch::new("xdg-entry-invalid");
        s.write("user/applications/nogroup.desktop", "[Other]\nName=x\n");
        s.write("user/applications/garbled.desktop", "[Desktop Entry]\nthis is not a pair\n");
        s.write("system/applications/nogroup.desktop", &desktop(""));
        s.write("system/applications/garbled.desktop", &desktop(""));
        let env = s.env(&["user", "system"]);
        assert_eq!(find(&env, "nogroup").unwrap_err(), Missing::Invalid("no [Desktop Entry] group".into()));
        match find(&env, "garbled").unwrap_err() {
            Missing::Invalid(reason) => assert!(reason.contains("line 2"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(find(&env, "absent").unwrap_err(), Missing::Unknown);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_invalid() {
        let s = Scratch::new("xdg-entry-unreadable");
        let big = format!("[Desktop Entry]\nName=x\n# {}\n", "x".repeat(MAX_BYTES as usize));
        s.write("share/applications/big.desktop", &big);
        std::fs::create_dir_all(s.path("share/applications")).unwrap();
        std::fs::write(s.path("share/applications/binary.desktop"), [0xff, 0xfe, 0x00]).unwrap();
        std::os::unix::fs::symlink(s.path("nowhere"), s.path("share/applications/dangling.desktop")).unwrap();
        let env = s.env(&["share"]);
        assert_eq!(find(&env, "big").unwrap_err(), Missing::Invalid("larger than 1 MiB".into()));
        assert_eq!(find(&env, "binary").unwrap_err(), Missing::Invalid("not UTF-8".into()));
        assert!(matches!(find(&env, "dangling").unwrap_err(), Missing::Invalid(r) if r.starts_with("cannot read it")));
    }

    #[test]
    fn each_filter_names_why_an_entry_is_unlisted() {
        let s = Scratch::new("xdg-entry-filter");
        let exe = s.path("bin/run");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plain = s.path("bin/plain");
        std::fs::write(&plain, "data").unwrap();
        let env = s.env(&["share"]);
        let cases: Vec<(String, Option<Filter>)> = vec![
            (desktop(""), None),
            ("[Desktop Entry]\nType=Link\nName=X\nExec=x\n".into(), Some(Filter::NotApplication)),
            ("[Desktop Entry]\nName=X\nExec=x\n".into(), Some(Filter::NotApplication)),
            ("[Desktop Entry]\nType=Application\nExec=x\n".into(), Some(Filter::NoName)),
            ("[Desktop Entry]\nType=Application\nName=X\n".into(), Some(Filter::NoExec)),
            (desktop("NoDisplay=true\n"), Some(Filter::NoDisplay)),
            (desktop("NoDisplay=false\n"), None),
            (desktop("OnlyShowIn=GNOME;KDE;\n"), Some(Filter::OnlyShowIn)),
            (desktop("OnlyShowIn=GNOME;Hyprland;\n"), None),
            (desktop("NotShowIn=Hyprland;\n"), Some(Filter::NotShowIn)),
            (desktop("NotShowIn=GNOME;\n"), None),
            (desktop(&format!("TryExec={}\n", exe.display())), None),
            (desktop(&format!("TryExec={}\n", plain.display())), Some(Filter::TryExec)),
            (desktop(&format!("TryExec={}\n", s.path("bin/missing").display())), Some(Filter::TryExec)),
            (desktop("TryExec=no-such-binary-anywhere\n"), None),
        ];
        for (text, want) in cases {
            let e = entry_in(&s, &env, &text);
            assert_eq!(e.unlisted(&env), want, "{text}");
        }
    }

    #[test]
    fn an_unset_desktop_never_matches_only_show_in() {
        let s = Scratch::new("xdg-entry-nodesktop");
        let mut env = s.env(&["share"]);
        let e = entry_in(&s, &env, &desktop("OnlyShowIn=Hyprland;\n"));
        assert_eq!(e.unlisted(&env), None);
        env.desktops.clear();
        assert_eq!(e.unlisted(&env), Some(Filter::OnlyShowIn));
        let e = entry_in(&s, &env, &desktop("NotShowIn=Hyprland;\n"));
        assert_eq!(e.unlisted(&env), None, "an unset desktop matches no NotShowIn either");
    }

    #[test]
    fn filter_names_are_the_documented_ones() {
        let all = [
            Filter::NotApplication,
            Filter::NoName,
            Filter::NoExec,
            Filter::NoDisplay,
            Filter::OnlyShowIn,
            Filter::NotShowIn,
            Filter::TryExec,
        ];
        let names: Vec<_> = all.iter().map(|f| f.as_str()).collect();
        assert_eq!(names, ["not-application", "no-name", "no-exec", "no-display", "only-show-in", "not-show-in", "try-exec"]);
    }

    #[test]
    fn localized_fields_follow_the_locale() {
        let s = Scratch::new("xdg-entry-locale");
        let mut env = s.env(&["share"]);
        let text = "[Desktop Entry]\nType=Application\nExec=x\nName=Files\nName[de]=Dateien\nName[de_AT]=Dateien AT\n\
                    GenericName[de]=Dateiverwaltung\nComment=Browse\nKeywords=files;folders;\nKeywords[de]=Dateien;Ordner;\n";
        assert_eq!(entry_in(&s, &env, text).name.as_deref(), Some("Files"));
        env.locale = Locale::parse("de_DE.UTF-8");
        let e = entry_in(&s, &env, text);
        assert_eq!(e.name.as_deref(), Some("Dateien"));
        assert_eq!(e.generic_name.as_deref(), Some("Dateiverwaltung"));
        assert_eq!(e.comment.as_deref(), Some("Browse"), "no German comment falls back to the plain one");
        assert_eq!(e.keywords, ["Dateien", "Ordner"]);
        env.locale = Locale::parse("de_AT");
        assert_eq!(entry_in(&s, &env, text).name.as_deref(), Some("Dateien AT"));
    }

    #[test]
    fn actions_keep_their_order_and_skip_a_group_without_a_name() {
        let s = Scratch::new("xdg-entry-actions");
        let env = s.env(&["share"]);
        let text = desktop(
            "Actions=second;nameless;first;absent;\n\n\
             [Desktop Action first]\nName=First\nExec=a --first\nIcon=one\n\n\
             [Desktop Action nameless]\nExec=a --nameless\n\n\
             [Desktop Action second]\nName=Second\nExec=a --second\n",
        );
        let e = entry_in(&s, &env, &text);
        let seen: Vec<_> = e.actions.iter().map(|a| (a.id.as_str(), a.name.as_str(), a.exec.as_deref())).collect();
        assert_eq!(seen, [("second", "Second", Some("a --second")), ("first", "First", Some("a --first"))]);
        assert_eq!(e.actions[1].icon.as_deref(), Some("one"));
    }

    #[test]
    fn exec_keeps_one_backslash_for_an_escaped_one() {
        let s = Scratch::new("xdg-entry-exec");
        let env = s.env(&["share"]);
        let e = entry_in(&s, &env, "[Desktop Entry]\nType=Application\nName=X\nExec=printf a\\\\b %U\n");
        assert_eq!(e.exec.as_deref(), Some("printf a\\b %U"));
    }

    #[test]
    fn the_remaining_fields_are_read_as_written() {
        let s = Scratch::new("xdg-entry-fields");
        let env = s.env(&["share"]);
        let e = entry_in(
            &s,
            &env,
            &desktop(
                "Icon=app-icon\nPath=/work\nTerminal=true\nDBusActivatable=true\nCategories=Utility;Network;\n\
                 StartupWMClass=AppClass\nHidden=false\n",
            ),
        );
        assert_eq!(e.icon.as_deref(), Some("app-icon"));
        assert_eq!(e.path.as_deref(), Some("/work"));
        assert!(e.terminal && e.dbus_activatable && !e.hidden);
        assert_eq!(e.categories, ["Utility", "Network"]);
        assert_eq!(e.startup_wm_class.as_deref(), Some("AppClass"));
        assert!(e.keywords.is_empty() && e.only_show_in.is_empty() && e.actions.is_empty());
    }

    #[test]
    fn symlinked_trees_are_followed_and_a_loop_ends() {
        let s = Scratch::new("xdg-entry-links");
        s.write("store/real.desktop", &desktop(""));
        std::fs::create_dir_all(s.path("share/applications")).unwrap();
        std::os::unix::fs::symlink(s.path("store"), s.path("share/applications/linked")).unwrap();
        std::os::unix::fs::symlink(s.path("share/applications"), s.path("store/back")).unwrap();
        let env = s.env(&["share"]);
        assert_eq!(ids(&env), ["linked-real"]);
    }
}
