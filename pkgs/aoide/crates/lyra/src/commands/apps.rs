//! `lyra apps` — the installed desktop apps and their resolved icons.
//!
//! - `apps list` builds the document fresh and prints it; it never reads the
//!   file.
//! - `apps show <id>` is one entry in the same shape, plus the `.desktop` file
//!   it came from, whether the shell lists it, and the filter that says why
//!   not.
//! - `apps publish` writes `song/stage/apps.json` (CONTRACTS.md §4, "apps.json
//!   v0"). With `--run` it keeps that file current until stopped: every 2 s it
//!   recomputes a [`fingerprint`] of everything the document is read from, and
//!   only a changed fingerprint rebuilds it. The planned `aoide-apps` user unit
//!   will run exactly that, so the shell reads a file instead of parsing
//!   `.desktop` files itself.
//!
//! Desktop entries and icon themes are read by `crate::xdg`, the workspace's
//! one freedesktop parser. The document carries what a launcher paints and
//! launches by id: never `Exec`, `Path` or `TryExec`, so what runs is decided
//! by the planned `lyra launch`, not by a file anyone could write into the
//! stage tree.
//! `lyra icon` is unrelated: it serves pinned Iconify glyphs.

use crate::dispatch::Invocation;
use crate::output::{io_cause, Fix, Kind, Outcome};
use crate::registry::{arg, cmd, flag, Registry};
use crate::xdg::entry::{self, Entry, Found, Missing};
use crate::xdg::icon_theme::Resolver;
use crate::xdg::Env;
use aoide_conduct::graph::clean_line;
use aoide_protocol::suggest::closest;
use aoide_protocol::Door;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

const FILE: &str = "apps.json";
const ICON_SIZE: u32 = 48;
const POLL: Duration = Duration::from_secs(2);

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["apps", "list"],
        summary: "List installed desktop apps with their resolved icons, exactly as the shell's apps.json carries them.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_list,
        examples: ["apps list", "apps list --json"],
        brief: "List installed desktop apps and their icons.",
    ));
    r.insert(cmd!(
        path: ["apps", "show"],
        summary: "Show one desktop app by id, in the apps.json shape plus its .desktop file, whether the shell lists it, and why not.",
        args: [arg!("id", "string", true, "A desktop-file id as `apps list` prints it.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_show,
        examples: ["apps show org.gnome.Nautilus"],
        brief: "Show one desktop app by id.",
    ));
    r.insert(cmd!(
        path: ["apps", "publish"],
        summary: "Write song/stage/apps.json, the app list the shell reads; rewritten only when the apps differ from the file on disk.",
        args: [],
        flags: [flag!(
            "run",
            "bool",
            "Keep song/stage/apps.json current until stopped: re-check every 2 s, rewrite only on change."
        )],
        gated: false,
        implemented: true,
        handler: handle_publish,
        examples: ["apps publish", "apps publish --run"],
        brief: "Write apps.json, or keep it current with --run.",
    ));
}

fn handle_list(_inv: &Invocation) -> Outcome {
    let env = Env::from_process();
    let doc = document(&env, &Resolver::new(&env), &aoide_storage::time::now_iso_utc());
    Outcome::ok("apps.list", list_text(&doc)).with_data(doc)
}

fn list_text(doc: &Value) -> String {
    let entries = doc["entries"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut lines = vec![format!("{} apps", entries.len())];
    for e in entries {
        let terminal = if e["terminal"] == true { " terminal" } else { "" };
        let (name, id) = (e["name"].as_str().unwrap_or_default(), e["id"].as_str().unwrap_or_default());
        lines.push(format!("  {} ({}){terminal}", clean_line(name), clean_line(id)));
    }
    lines.join("\n")
}

fn handle_show(inv: &Invocation) -> Outcome {
    let env = Env::from_process();
    show(&env, &Resolver::new(&env), inv.args.first().map(String::as_str).unwrap_or_default())
}

fn handle_publish(inv: &Invocation) -> Outcome {
    let cmd = "apps.publish";
    if inv.flag_present("run") && inv.door != Door::Cli {
        return Outcome::refuse(
            cmd,
            Kind::Usage,
            "`lyra apps publish --run` is CLI-only",
            "it never returns, so over another door it would hold the call open forever",
            Fix::Run("lyra apps publish".into()),
        );
    }
    let env = Env::from_process();
    let path = aoide_storage::fs::stage_dir().join(FILE);
    if inv.flag_present("run") {
        run(&env, &path);
    }
    match publish(&env, &path, &aoide_storage::time::now_iso_utc()) {
        Ok(p) => {
            let verb = if p.written { "wrote" } else { "unchanged" };
            Outcome::ok(cmd, format!("{verb} {} ({} apps)", path.display(), p.entries)).with_data(json!({
                "path": path.display().to_string(),
                "written": p.written,
                "entries": p.entries,
                "theme": p.theme,
            }))
        }
        Err(e) => {
            let cause = io_cause("write", &path, &e);
            Outcome::refuse(cmd, Kind::Failed, "apps.json was not written", cause.why, Fix::None("the stage directory must be writable"))
                .with_detail(cause.detail)
        }
    }
}

fn show(env: &Env, resolver: &Resolver, id: &str) -> Outcome {
    let cmd = "apps.show";
    let e = match entry::find(env, id) {
        Ok(e) => e,
        Err(missing) => return refuse(cmd, env, id, missing),
    };
    let filter = e.unlisted(env);
    let mut doc = entry_json(&e, resolver);
    doc["file"] = json!(e.file.display().to_string());
    doc["listed"] = json!(filter.is_none());
    doc["filtered"] = json!(filter.map(|f| f.as_str()));
    let note = filter.map(|f| format!(", unlisted: {}", f.as_str())).unwrap_or_default();
    let message = format!(
        "{} ({}){note} from {}",
        clean_line(e.name.as_deref().unwrap_or_default()),
        clean_line(&e.id),
        clean_line(&e.file.display().to_string())
    );
    Outcome::ok(cmd, message).with_data(doc)
}

fn refuse(cmd: &str, env: &Env, id: &str, missing: Missing) -> Outcome {
    let why = match missing {
        Missing::Unknown => {
            let ids = entry::ids(env);
            let near = closest(id, ids.iter().map(String::as_str), 3);
            let hint = near.iter().map(|id| format!("`{}`", clean_line(id))).collect::<Vec<_>>().join(", ");
            let hint = if near.is_empty() { String::new() } else { format!("; did you mean {hint}?") };
            format!("no desktop entry has that id{hint}")
        }
        Missing::Hidden => "the entry is marked Hidden, which deletes it".to_string(),
        Missing::Invalid(reason) => reason,
    };
    Outcome::refuse(cmd, Kind::Refused, format!("no app `{}`", clean_line(id)), why, Fix::Run("lyra apps list".into()))
}

fn icon_path(resolver: &Resolver, icon: Option<&str>) -> Option<String> {
    icon.and_then(|i| resolver.resolve(i, ICON_SIZE)).map(|p| p.to_string_lossy().into_owned())
}

fn entry_json(e: &Entry, resolver: &Resolver) -> Value {
    let actions: Vec<Value> = e
        .actions
        .iter()
        .filter(|a| a.exec.is_some())
        .map(|a| json!({ "id": a.id, "name": a.name, "icon": icon_path(resolver, a.icon.as_deref()) }))
        .collect();
    json!({
        "id": e.id,
        "name": e.name.clone().unwrap_or_default(),
        "genericName": e.generic_name,
        "comment": e.comment,
        "icon": icon_path(resolver, e.icon.as_deref()),
        "keywords": e.keywords,
        "categories": e.categories,
        "terminal": e.terminal,
        "startupWMClass": e.startup_wm_class,
        "actions": actions,
    })
}

fn document(env: &Env, resolver: &Resolver, at: &str) -> Value {
    let mut listed: Vec<Entry> = entry::scan(env)
        .into_iter()
        .filter_map(|f| match f {
            Found::Entry(e) if !e.hidden && e.unlisted(env).is_none() => Some(*e),
            _ => None,
        })
        .collect();
    listed.sort_by_cached_key(|e| (e.name.clone().unwrap_or_default().to_lowercase(), e.id.clone()));
    json!({
        "schemaVersion": "0",
        "at": at,
        "theme": resolver.theme(),
        "iconSize": ICON_SIZE,
        "fallbackIcon": icon_path(resolver, Some("application-x-executable")),
        "entries": listed.iter().map(|e| entry_json(e, resolver)).collect::<Vec<_>>(),
    })
}

struct Published {
    written: bool,
    entries: usize,
    theme: String,
}

/// Replaces `path` with a fresh document only when it differs from the one on disk in anything but `at`.
fn publish(env: &Env, path: &Path, at: &str) -> std::io::Result<Published> {
    let doc = document(env, &Resolver::new(env), at);
    let mut published = Published {
        written: false,
        entries: doc["entries"].as_array().map_or(0, Vec::len),
        theme: doc["theme"].as_str().unwrap_or_default().to_string(),
    };
    let on_disk = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    if on_disk.is_some_and(|d| without_at(d) == without_at(doc.clone())) {
        return Ok(published);
    }
    aoide_storage::fs::atomic_write(path, &format!("{}\n", serde_json::to_string_pretty(&doc)?))?;
    published.written = true;
    Ok(published)
}

fn without_at(mut doc: Value) -> Value {
    if let Some(map) = doc.as_object_mut() {
        map.remove("at");
    }
    doc
}

fn run(env: &Env, path: &Path) -> ! {
    let mut last: Option<Vec<String>> = None;
    loop {
        let now = fingerprint(env);
        if last.as_ref() != Some(&now) {
            match publish(env, path, &aoide_storage::time::now_iso_utc()) {
                Ok(p) => {
                    if p.written {
                        eprintln!("lyra apps: wrote {} ({} apps)", path.display(), p.entries);
                    }
                    last = Some(now);
                }
                Err(e) => eprintln!("lyra apps: {}", io_cause("write", path, &e).why),
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Everything the document is built from, as lines that compare equal while nothing changed.
///
/// Stamped, each by canonical path:
/// - every data dir, and in it each `*.desktop` file (size, mtime);
/// - `gtk-3.0/settings.ini` (size, mtime);
/// - every `<base>/<theme>` the icon lookup chain asks for (the configured
///   theme, its `Inherits`, `hicolor`), in every icon base dir, present or not
///   (directory mtime), and in each one that exists its `index.theme` (size,
///   mtime) and each subdir the index lists (directory mtime, 0 when absent: a
///   file added to a directory bumps it; a subdir is not canonicalized);
/// - the loose-icon dirs: each icon base dir and each `pixmaps` dir (directory
///   mtime).
///
/// Icon files themselves and `.desktop` files outside the data dirs'
/// `applications/` are not stamped. Every path is re-resolved on every call
/// and nothing here holds an inode: a nix profile swap shows only as a new
/// canonical path.
fn fingerprint(env: &Env) -> Vec<String> {
    let mut lines = Vec::new();
    for dir in &env.data_dirs {
        let Ok(canonical) = std::fs::canonicalize(dir) else {
            lines.push(format!("{} absent", dir.display()));
            continue;
        };
        lines.push(canonical.display().to_string());
        for (rel, file) in entry::desktop_files(&canonical.join("applications")) {
            lines.push(format!("{rel} {}", stamp(&file)));
        }
    }
    let settings = env.config_home.join("gtk-3.0/settings.ini");
    lines.push(format!("settings.ini {}", stamp(&settings)));
    let inputs = Resolver::inputs(env);
    for theme in &inputs.themes {
        lines.push(format!("icon-theme {} {}", theme.dir.display(), dir_stamp(&theme.dir)));
        if theme.dir.is_dir() {
            lines.push(format!("icon-index {} {}", theme.index.display(), stamp(&theme.index)));
            lines.extend(theme.subdirs.iter().map(|d| format!("icon-dir {} {}", d.display(), mtime(d))));
        }
    }
    lines.extend(inputs.loose.iter().map(|d| format!("icon-loose {} {}", d.display(), dir_stamp(d))));
    lines
}

/// The link target is part of the stamp: a store file always has mtime 1, and a re-pointed link can keep the size.
fn stamp(path: &Path) -> String {
    let target = std::fs::canonicalize(path).map_or_else(|_| "unresolved".to_string(), |p| p.display().to_string());
    let Ok(meta) = std::fs::metadata(path) else { return format!("{target} absent") };
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    format!("{target} {} {modified}", meta.len())
}

fn dir_stamp(path: &Path) -> String {
    let target = std::fs::canonicalize(path).map_or_else(|_| "unresolved".to_string(), |p| p.display().to_string());
    format!("{target} {}", mtime(path))
}

/// Directory mtime in nanoseconds, 0 when absent: a file added to the directory bumps it.
fn mtime(path: &Path) -> u128 {
    let modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
    modified.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xdg::fixture::{desktop, Scratch};
    use aoide_protocol::output::Status;
    use std::collections::BTreeMap;

    fn app(name: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName={name}\nExec=run-{name}\n")
    }

    fn build(env: &Env, at: &str) -> Value {
        document(env, &Resolver::new(env), at)
    }

    fn ids_of(doc: &Value) -> Vec<&str> {
        doc["entries"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect()
    }

    #[test]
    fn the_document_has_every_key_and_entries_sorted_by_name_then_id() {
        let s = Scratch::new("apps-doc");
        s.write("share/applications/zed.desktop", &app("Apple"));
        s.write("share/applications/b-banana.desktop", &app("banana"));
        s.write("share/applications/a-apple.desktop", &app("apple"));
        s.write("share/applications/hidden.desktop", &desktop("Hidden=true\n"));
        s.write("share/applications/quiet.desktop", &desktop("NoDisplay=true\n"));
        s.write("share/applications/broken.desktop", "garbage");
        let doc = build(&s.env(&["share"]), "2026-10-07T00:00:00Z");
        assert_eq!(doc["schemaVersion"], "0");
        assert_eq!(doc["at"], "2026-10-07T00:00:00Z");
        assert_eq!(doc["theme"], "hicolor");
        assert_eq!(doc["iconSize"], 48);
        assert_eq!(doc["fallbackIcon"], Value::Null);
        assert_eq!(ids_of(&doc), ["a-apple", "zed", "b-banana"], "case-folded name, then id; unlisted and invalid ids are absent");
        for e in doc["entries"].as_array().unwrap() {
            let keys: Vec<&str> = e.as_object().unwrap().keys().map(String::as_str).collect();
            let mut want = ["actions", "categories", "comment", "genericName", "icon", "id", "keywords", "name", "startupWMClass", "terminal"];
            want.sort();
            let mut got = keys.clone();
            got.sort();
            assert_eq!(got, want, "{e}");
        }
    }

    #[test]
    fn an_entry_carries_its_resolved_icons_and_launchable_actions_only() {
        let s = Scratch::new("apps-entry");
        s.write("share/icons/hicolor/index.theme", "[Icon Theme]\nName=hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\n");
        let icon = s.touch("share/icons/hicolor/48x48/apps/tool.png");
        let other = s.touch("share/icons/hicolor/48x48/apps/other.png");
        s.touch("share/icons/hicolor/48x48/apps/application-x-executable.png");
        s.write(
            "share/applications/tool.desktop",
            &desktop(
                "Icon=tool\nGenericName=Tool\nComment=Does things\nKeywords=a;b;\nCategories=Utility;\nTerminal=true\nStartupWMClass=ToolClass\n\
                 Actions=go;nameless;noexec;\n\n[Desktop Action go]\nName=Go\nExec=tool go\nIcon=other\n\n\
                 [Desktop Action nameless]\nExec=tool nameless\n\n[Desktop Action noexec]\nName=No Exec\n",
            ),
        );
        let doc = build(&s.env(&["share"]), "t");
        assert!(doc["fallbackIcon"].as_str().unwrap().ends_with("application-x-executable.png"));
        let e = &doc["entries"][0];
        assert_eq!(e["icon"], icon.display().to_string());
        assert_eq!(e["genericName"], "Tool");
        assert_eq!(e["comment"], "Does things");
        assert_eq!(e["keywords"], json!(["a", "b"]));
        assert_eq!(e["categories"], json!(["Utility"]));
        assert_eq!(e["terminal"], true);
        assert_eq!(e["startupWMClass"], "ToolClass");
        assert_eq!(e["actions"], json!([{ "id": "go", "name": "Go", "icon": other.display().to_string() }]));
    }

    #[test]
    fn the_document_never_carries_exec_path_or_tryexec() {
        let s = Scratch::new("apps-secret");
        s.write(
            "share/applications/tool.desktop",
            "[Desktop Entry]\nType=Application\nName=Tool\nExec=hush-binary --hush-flag\nPath=/hush/dir\nTryExec=/bin/sh\n\
             Actions=go;\n\n[Desktop Action go]\nName=Go\nExec=hush-action\n",
        );
        let env = s.env(&["share"]);
        let text = serde_json::to_string(&build(&env, "t")).unwrap();
        assert!(text.contains("\"Tool\""), "the entry is listed: {text}");
        for needle in ["hush", "\"Exec\"", "\"exec\"", "\"Path\"", "\"path\"", "TryExec", "tryExec"] {
            assert!(!text.contains(needle), "{needle} leaked into {text}");
        }
        let shown = show(&env, &Resolver::new(&env), "tool").data.unwrap();
        let text = serde_json::to_string(&shown).unwrap();
        for needle in ["hush", "\"Exec\"", "\"exec\"", "\"Path\"", "TryExec", "tryExec"] {
            assert!(!text.contains(needle), "{needle} leaked into show: {text}");
        }
    }

    #[test]
    fn the_fingerprint_follows_desktop_file_changes_and_ignores_stray_files() {
        let s = Scratch::new("apps-print");
        let env = s.env(&["share"]);
        let file = s.write("share/applications/a.desktop", &app("a"));
        let before = fingerprint(&env);
        assert_eq!(fingerprint(&env), before, "stable while nothing changes");

        s.write("share/applications/b.desktop", &app("b"));
        let added = fingerprint(&env);
        assert_ne!(added, before, "a file added");
        std::fs::write(&file, format!("{}# longer\n", app("a"))).unwrap();
        let modified = fingerprint(&env);
        assert_ne!(modified, added, "a file modified");
        std::fs::remove_file(s.path("share/applications/b.desktop")).unwrap();
        assert_ne!(fingerprint(&env), modified, "a file removed");

        s.touch("share/applications/notes.txt");
        let settled = fingerprint(&env);
        s.touch("share/applications/more.txt");
        s.touch("share/other/x.png");
        assert_eq!(fingerprint(&env), settled, "files that are not desktop entries or icon inputs do not count");
    }

    #[test]
    fn an_icon_landing_after_its_desktop_file_changes_the_fingerprint_and_the_republished_document() {
        let s = Scratch::new("apps-print-icon");
        s.write("share/icons/hicolor/index.theme", "[Icon Theme]\nName=hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\n");
        s.write("share/applications/tool.desktop", &desktop_with_icon("tool"));
        let env = s.env(&["share"]);
        let path = s.path("stage/apps.json");
        publish(&env, &path, "t1").unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["entries"][0]["icon"], Value::Null);
        let before = fingerprint(&env);
        assert_eq!(fingerprint(&env), before);

        let icon = s.touch("share/icons/hicolor/48x48/apps/tool.png");
        assert_ne!(fingerprint(&env), before, "the icon landed in a listed subdir");
        assert!(publish(&env, &path, "t2").unwrap().written);
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["entries"][0]["icon"], icon.display().to_string());
    }

    #[test]
    fn a_new_theme_a_missing_subdir_appearing_and_a_pixmap_all_change_the_fingerprint() {
        let s = Scratch::new("apps-print-theme");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=late\n");
        s.write("share/icons/hicolor/index.theme", "[Icon Theme]\nName=hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\n");
        let env = s.env(&["share"]);
        let before = fingerprint(&env);
        assert_eq!(Resolver::new(&env).theme(), "hicolor");

        std::fs::create_dir_all(s.path("share/icons/hicolor/48x48/apps")).unwrap();
        let subdir = fingerprint(&env);
        assert_ne!(subdir, before, "a listed subdir that did not exist");
        s.write("share/icons/late/index.theme", "[Icon Theme]\nName=late\nInherits=deep\nDirectories=16x16/apps\n\n[16x16/apps]\nSize=16\nType=Fixed\n");
        let theme = fingerprint(&env);
        assert_ne!(theme, subdir, "the configured theme gets an index.theme");
        assert_eq!(Resolver::new(&env).theme(), "late");
        s.write("share/icons/deep/index.theme", "[Icon Theme]\nName=deep\nDirectories=\n");
        let parent = fingerprint(&env);
        assert_ne!(parent, theme, "an Inherits parent that did not exist");
        s.touch("share/pixmaps/tool.xpm");
        assert_ne!(fingerprint(&env), parent, "a loose pixmap");
    }

    fn desktop_with_icon(icon: &str) -> String {
        format!("{}Icon={icon}\n", app("Tool"))
    }

    #[test]
    fn the_fingerprint_sees_a_per_file_link_retargeted_to_a_file_of_the_same_size_and_mtime() {
        let s = Scratch::new("apps-print-relink");
        let one = s.write("store1/a.desktop", &app("Alpha"));
        let two = s.write("store2/a.desktop", &app("Bravo"));
        assert_eq!(one.metadata().unwrap().len(), two.metadata().unwrap().len());
        let epoch = UNIX_EPOCH + Duration::from_secs(1);
        for f in [&one, &two] {
            std::fs::File::options().write(true).open(f).unwrap().set_modified(epoch).unwrap();
        }
        std::fs::create_dir_all(s.path("share/applications")).unwrap();
        let link = s.path("share/applications/a.desktop");
        std::os::unix::fs::symlink(&one, &link).unwrap();
        let env = s.env(&["share"]);
        let before = fingerprint(&env);
        assert_eq!(build(&env, "t")["entries"][0]["name"], "Alpha");

        std::os::unix::fs::symlink(&two, s.path("share/applications/a.new")).unwrap();
        std::fs::rename(s.path("share/applications/a.new"), &link).unwrap();
        assert_ne!(fingerprint(&env), before);
        assert_eq!(build(&env, "t")["entries"][0]["name"], "Bravo");
    }

    #[test]
    fn untrusted_names_are_cleaned_before_they_reach_a_terminal_or_a_message() {
        let s = Scratch::new("apps-clean");
        let evil = "Calc\\n  Forged (fake)\\n\u{1b}]0;owned\u{7}";
        s.write("share/applications/evil.desktop", &format!("[Desktop Entry]\nType=Application\nName={evil}\nExec=x\n"));
        let env = s.env(&["share"]);
        let name = build(&env, "t")["entries"][0]["name"].as_str().unwrap().to_string();
        assert!(name.contains('\n') && name.contains('\u{1b}'), "the data keeps the raw text: {name:?}");
        let message = show(&env, &Resolver::new(&env), "evil").message;
        assert_eq!(message.lines().count(), 1, "{message:?}");
        assert!(!message.chars().any(char::is_control), "{message:?}");
        let list = list_text(&build(&env, "t"));
        assert_eq!(list.lines().count(), 2, "a count line and one entry line: {list:?}");
        assert!(!list.replace('\n', "").chars().any(char::is_control), "{list:?}");
    }

    #[test]
    fn a_refusal_cleans_the_ids_it_suggests_and_the_id_it_echoes() {
        let s = Scratch::new("apps-clean-near");
        s.write("share/applications/firefox\n\u{1b}.desktop", &app("Evil"));
        let env = s.env(&["share"]);
        let o = show(&env, &Resolver::new(&env), "firefoxes");
        let why = o.data.as_ref().unwrap()["refusal"]["why"].as_str().unwrap();
        assert!(why.contains("did you mean `firefox`"), "a suggestion was made: {why:?}");
        for text in [why, o.message.as_str()] {
            assert!(!text.chars().any(char::is_control), "{text:?}");
        }
    }

    #[test]
    fn the_fingerprint_sees_the_settings_file_and_a_data_dir_appearing() {
        let s = Scratch::new("apps-print-settings");
        let env = s.env(&["share", "later"]);
        let before = fingerprint(&env);
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=a\n");
        let with_settings = fingerprint(&env);
        assert_ne!(with_settings, before);
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=bb\n");
        let edited = fingerprint(&env);
        assert_ne!(edited, with_settings);
        s.write("later/applications/a.desktop", &app("a"));
        assert_ne!(fingerprint(&env), edited);
    }

    #[test]
    fn a_data_dir_symlink_swapped_atomically_changes_the_fingerprint_and_the_listing() {
        let s = Scratch::new("apps-swap");
        s.write("gen1/applications/one.desktop", &app("one"));
        s.write("gen2/applications/two.desktop", &app("two"));
        std::os::unix::fs::symlink(s.path("gen1"), s.path("cur")).unwrap();
        let env = s.env(&["cur"]);
        let before = fingerprint(&env);
        assert_eq!(ids_of(&build(&env, "t")), ["one"]);

        std::os::unix::fs::symlink(s.path("gen2"), s.path("cur.new")).unwrap();
        std::fs::rename(s.path("cur.new"), s.path("cur")).unwrap();
        assert_ne!(fingerprint(&env), before);
        assert_eq!(ids_of(&build(&env, "t")), ["two"]);
    }

    #[test]
    fn publishing_twice_writes_once_and_an_at_only_change_writes_nothing() {
        let s = Scratch::new("apps-publish");
        let env = s.env(&["share"]);
        s.write("share/applications/a.desktop", &app("a"));
        let path = s.path("stage/apps.json");

        let first = publish(&env, &path, "t1").unwrap();
        assert!(first.written);
        assert_eq!((first.entries, first.theme.as_str()), (1, "hicolor"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("}\n") && text.contains("\"at\": \"t1\""));

        let again = publish(&env, &path, "t2").unwrap();
        assert!(!again.written, "only `at` differs");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text, "the file is untouched");

        s.write("share/applications/b.desktop", &app("b"));
        let changed = publish(&env, &path, "t3").unwrap();
        assert!(changed.written);
        assert_eq!(changed.entries, 2);
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["at"], "t3");

        std::fs::write(&path, "not json").unwrap();
        assert!(publish(&env, &path, "t4").unwrap().written, "an unreadable file is replaced");
    }

    #[test]
    fn show_gives_the_entry_its_file_and_whether_it_is_listed() {
        let s = Scratch::new("apps-show");
        s.write("share/applications/shown.desktop", &app("Shown"));
        s.write("share/applications/quiet.desktop", &desktop("NoDisplay=true\n"));
        let env = s.env(&["share"]);
        let r = Resolver::new(&env);

        let ok = show(&env, &r, "shown");
        assert_eq!(ok.status, Status::Ok);
        let data = ok.data.unwrap();
        assert_eq!((data["listed"].clone(), data["filtered"].clone()), (json!(true), Value::Null));
        assert_eq!(data["file"], s.path("share/applications/shown.desktop").display().to_string());
        assert_eq!(data["name"], "Shown");

        let quiet = show(&env, &r, "quiet").data.unwrap();
        assert_eq!((quiet["listed"].clone(), quiet["filtered"].clone()), (json!(false), json!("no-display")));
    }

    #[test]
    fn show_refuses_unknown_hidden_and_invalid_ids_with_the_closest_ids() {
        let s = Scratch::new("apps-refuse");
        s.write("user/applications/gone.desktop", &desktop("Hidden=true\n"));
        s.write("system/applications/gone.desktop", &desktop(""));
        s.write("system/applications/firefox.desktop", &app("Firefox"));
        s.write("system/applications/broken.desktop", "[Other]\n");
        let env = s.env(&["user", "system"]);
        let r = Resolver::new(&env);
        let refusal = |id: &str| {
            let o = show(&env, &r, id);
            assert_eq!(o.status, Status::Error, "{id}: a refusal by the world exits 1");
            assert_eq!(o.data.as_ref().unwrap()["refusal"]["fix"]["run"], "lyra apps list");
            o.data.unwrap()["refusal"]["why"].as_str().unwrap().to_string()
        };
        assert_eq!(refusal("gone"), "the entry is marked Hidden, which deletes it");
        assert_eq!(refusal("broken"), "no [Desktop Entry] group");
        assert!(refusal("firefx").contains("did you mean `firefox`?"), "{}", refusal("firefx"));
        assert_eq!(refusal("no-such-app-xyz"), "no desktop entry has that id");
    }

    #[test]
    fn apps_publish_run_is_refused_over_another_door() {
        let inv = |door| Invocation {
            path: vec!["apps".into(), "publish".into()],
            args: vec![],
            flags: BTreeMap::from([("run".to_string(), "true".to_string())]),
            door,
        };
        let o = handle_publish(&inv(Door::Mcp));
        assert_eq!(o.status, Status::Usage);
        assert!(o.message.contains("CLI-only"), "{}", o.message);
    }
}
