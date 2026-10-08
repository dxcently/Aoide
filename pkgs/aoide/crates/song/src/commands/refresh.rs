//! `rice refresh` — a built-in song takes the repo's changes to the files the
//! machine never edited, and keeps the rest.
//!
//! The activation seed copies a built-in song into the machine songbook once and
//! never overwrites, so a fix shipped in the templates would never arrive. This
//! is the three-way rule that lets it arrive without clobbering an edit. Per
//! file: S = the shipped templates copy, M = the machine copy, R = the hash this
//! command last wrote, kept in `aoide_storage::fs::songbook_record(song)` —
//! outside the song folder, so the §7.5 folder-equality gate, `rice declare`,
//! `rice list`, `rice take` and the widget sync never see it.
//!
//! ```text
//! S present, M = S                  in-sync         record S
//! S present, M != S, R = M          stale           replace, record S
//! S present, M != S, R != M         edited          keep, R unchanged
//! S present, M != S, no R           unrecorded      keep, no entry
//! S present, M absent, no R         new             add, record S
//! S present, M absent, R            machine-deleted keep absent, R unchanged
//! S absent,  M present, R = M       gone            delete, prune empty parents, drop R
//! S absent,  M present, R != M      edited          keep, drop R
//! S absent,  M present, no R        (machine-only, untouched, not listed)
//! ```
//!
//! Top-level `takes/` and `drafts/` ([`MACHINE_RUNTIME_DIRS`]) and machine-side
//! symlinks are never touched. A symlink inside the shipped copy is an error.

use crate::widgets::{read_builtin, MACHINE_RUNTIME_DIRS};
use aoide_protocol::output::{io_cause, serde_cause, Fix, Kind, Outcome, Status};
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use aoide_protocol::Invocation;
use aoide_storage::fs as shellbridge;
use aoide_storage::seal::{hex_encode, sha256};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["rice", "refresh"],
        summary: "Bring a built-in song's machine copy up to the shipped one: files the machine never edited take the repo's changes, edited files are kept. No <name>: every song in the shipped builtin.json. --check writes nothing and reports each file's state.",
        args: [arg!("name", "string", false, "Built-in song to refresh; defaults to every song the shipped builtin.json lists.")],
        flags: [
            flag!("check", "bool", "Report what would change, write nothing."),
            flag!("json", "bool", "Machine-readable output."),
        ],
        gated: false,
        implemented: true,
        handler: handle_rice_refresh,
        examples: ["rice refresh --check", "rice refresh sonata"],
    ));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    InSync,
    Stale,
    New,
    Gone,
    Edited,
    Unrecorded,
    MachineDeleted,
}

impl State {
    fn word(self) -> &'static str {
        match self {
            State::InSync => "in-sync",
            State::Stale => "stale",
            State::New => "new",
            State::Gone => "gone",
            State::Edited => "edited",
            State::Unrecorded => "unrecorded",
            State::MachineDeleted => "machine-deleted",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    None,
    Replace,
    Add,
    Delete,
    Keep,
}

impl Action {
    fn word(self) -> &'static str {
        match self {
            Action::None => "none",
            Action::Replace => "replace",
            Action::Add => "add",
            Action::Delete => "delete",
            Action::Keep => "keep",
        }
    }
}

struct Entry {
    path: String,
    state: State,
    action: Action,
}

pub(crate) struct SongReport {
    song: String,
    skipped: Option<String>,
    seeded: bool,
    entries: Vec<Entry>,
    modes: usize,
}

impl SongReport {
    fn to_json(&self) -> Value {
        json!({
            "song": self.song,
            "skipped": self.skipped,
            "seeded": self.seeded,
            "modesFixed": self.modes,
            "files": self.entries.iter().map(|e| json!({
                "path": e.path, "state": e.state.word(), "action": e.action.word(),
            })).collect::<Vec<_>>(),
        })
    }
}

fn handle_rice_refresh(inv: &Invocation) -> Outcome {
    let check = inv.flag_present("check");
    let Some(templates) = shellbridge::song_templates_dir() else {
        return Outcome::refuse(
            "rice.refresh",
            Kind::Refused,
            "no shipped songbook to refresh from",
            "AOIDE_SONG_TEMPLATES is unset and no share/lyra/songbook sits beside the binary",
            Fix::Set("AOIDE_SONG_TEMPLATES".into()),
        );
    };
    let names: Vec<String> = match inv.args.first() {
        Some(name) => {
            if !crate::compose::valid_song_name(name) || !templates.join(name).is_dir() {
                return Outcome::refuse(
                    "rice.refresh",
                    Kind::Refused,
                    format!("`{name}` is not shipped in {}", templates.display()),
                    "refresh only brings in a song the shipped songbook carries",
                    Fix::Run("lyra rice refresh --check".into()),
                );
            }
            vec![name.clone()]
        }
        None => match read_builtin(&templates) {
            Ok(b) => b.songs,
            Err(e) => return Outcome::error("rice.refresh", e.error),
        },
    };

    let (mut reports, mut errors) = (Vec::new(), Vec::new());
    shellbridge::with_stage_lock(|| {
        for name in &names {
            match refresh_song(name, &templates, check) {
                Ok(report) => reports.push(report),
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
    });

    let mut lines = Vec::new();
    let mut changed = Vec::new();
    for r in &reports {
        if let Some(why) = &r.skipped {
            lines.push(format!("{}: skipped, {why}", r.song));
            continue;
        }
        let acts = |a: Action| r.entries.iter().filter(|e| e.action == a).count();
        let kept = r.entries.iter().filter(|e| e.action == Action::Keep).count();
        let seeded = if r.seeded { "seeded whole, " } else { "" };
        lines.push(format!(
            "{}: {seeded}{} replaced, {} added, {} deleted, {kept} kept, {} in sync",
            r.song,
            acts(Action::Replace),
            acts(Action::Add),
            acts(Action::Delete),
            r.entries.iter().filter(|e| e.state == State::InSync).count(),
        ));
        for e in r.entries.iter().filter(|e| e.state != State::InSync) {
            lines.push(format!("  {:<15} {:<7} {}/{}", e.state.word(), e.action.word(), r.song, e.path));
            if !check && matches!(e.action, Action::Replace | Action::Add | Action::Delete) {
                changed.push(format!("{}/{}", r.song, e.path));
            }
        }
    }
    for e in &errors {
        lines.push(format!("error: {e}"));
    }
    let verb = if check { "checked" } else { "refreshed" };
    let message = if lines.is_empty() {
        format!("nothing built in to {}", if check { "check" } else { "refresh" })
    } else {
        format!("{verb} {} song(s)\n{}", reports.len(), lines.join("\n"))
    };
    let status = if errors.is_empty() { Status::Ok } else { Status::Error };
    Outcome::new("rice.refresh", status, message)
        .changed(changed)
        .with_data(json!({
            "check": check,
            "songs": reports.iter().map(SongReport::to_json).collect::<Vec<_>>(),
            "errors": errors,
        }))
}

/// Every regular file under `root` as `relpath -> absolute path`, skipping the
/// top-level machine-runtime dirs. `Err` names a symlink when `symlinks_err`.
fn walk(root: &Path, symlinks_err: bool) -> Result<BTreeMap<String, PathBuf>, String> {
    fn go(
        root: &Path,
        dir: &Path,
        symlinks_err: bool,
        out: &mut BTreeMap<String, PathBuf>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
            let path = entry.path();
            let rel = path.strip_prefix(root).expect("under root");
            if rel.components().count() == 1
                && MACHINE_RUNTIME_DIRS.contains(&rel.to_string_lossy().as_ref())
            {
                continue;
            }
            let ft = entry.file_type().map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
            if ft.is_symlink() {
                if symlinks_err {
                    return Err(format!("{} is a symlink; the shipped copy holds none", path.display()));
                }
            } else if ft.is_dir() {
                go(root, &path, symlinks_err, out)?;
            } else {
                out.insert(rel.to_string_lossy().into_owned(), path);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    go(root, root, symlinks_err, &mut out)?;
    Ok(out)
}

fn hash_file(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(|b| hex_encode(&sha256(&b)))
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Give the owner write on every file and dir lacking it (a store copy is
/// 0444/0555). Returns how many entries needed it; symlinks are left alone.
fn fix_modes(path: &Path, apply: bool) -> Result<usize, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Ok(0);
    }
    let mode = meta.permissions().mode();
    let mut fixed = 0;
    if mode & 0o200 == 0 {
        fixed += 1;
        if apply {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o200))
                .map_err(|e| format!("cannot chmod {}: {e}", path.display()))?;
        }
    }
    if meta.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| format!("cannot read {}: {e}", path.display()))? {
            let entry = entry.map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            fixed += fix_modes(&entry.path(), apply)?;
        }
    }
    Ok(fixed)
}

type Record = BTreeMap<String, String>;

fn read_record(song: &str) -> Result<Record, String> {
    let path = shellbridge::songbook_record(song);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Record::new()),
        Err(e) => return Err(io_cause("read", &path, &e).why),
    };
    let doc: Value = serde_json::from_str(&raw).map_err(|e| serde_cause(&path, &e).why)?;
    Ok(doc
        .get("files")
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default())
}

fn write_record(song: &str, source: &Path, files: &Record) -> Result<(), String> {
    let path = shellbridge::songbook_record(song);
    let doc = json!({
        "schemaVersion": "0", "song": song, "source": source.to_string_lossy(), "files": files,
    });
    let mut text = serde_json::to_string_pretty(&doc).expect("record serializes");
    text.push('\n');
    if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_cause("create", parent, &e).why)?;
    }
    shellbridge::atomic_write_bytes(&path, text.as_bytes()).map_err(|e| io_cause("write", &path, &e).why)
}

fn prune_empty_parents(root: &Path, mut dir: &Path) {
    while dir != root && std::fs::remove_dir(dir).is_ok() {
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
}

pub(crate) fn refresh_song(song: &str, templates: &Path, check: bool) -> Result<SongReport, String> {
    shellbridge::with_stage_lock(|| refresh_song_locked(song, templates, check))
}

fn refresh_song_locked(song: &str, templates: &Path, check: bool) -> Result<SongReport, String> {
    let source = templates.join(song);
    let machine = shellbridge::songbook_dir(song);
    let mut report = SongReport { song: song.into(), skipped: None, seeded: false, entries: vec![], modes: 0 };

    let store = walk(&source, true)?;
    let store_hashes: BTreeMap<&String, String> =
        store.iter().map(|(rel, p)| hash_file(p).map(|h| (rel, h))).collect::<Result<_, _>>()?;

    match std::fs::symlink_metadata(&machine) {
        Ok(m) if m.file_type().is_symlink() => {
            report.skipped = Some(format!("{} is a symlink", machine.display()));
            return Ok(report);
        }
        Ok(m) if !m.is_dir() => return Err(format!("{} is not a directory", machine.display())),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            report.seeded = true;
            report.entries = store
                .keys()
                .map(|p| Entry { path: p.clone(), state: State::New, action: Action::Add })
                .collect();
            if check {
                return Ok(report);
            }
            let tmp = machine.with_extension(format!("seed-tmp.{}", std::process::id()));
            let seeded = shellbridge::copy_dir_recursive(&source, &tmp)
                .map_err(|e| e.to_string())
                .and_then(|()| fix_modes(&tmp, true).map(|n| report.modes = n))
                .and_then(|()| std::fs::rename(&tmp, &machine).map_err(|e| e.to_string()));
            if let Err(e) = seeded {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(format!("failed to seed songbook `{song}` from {}: {e}", source.display()));
            }
            let record = store_hashes.iter().map(|(k, h)| ((*k).clone(), h.clone())).collect();
            write_record(song, &source, &record)?;
            return Ok(report);
        }
        Err(e) => return Err(io_cause("stat", &machine, &e).why),
    }

    report.modes = fix_modes(&machine, !check)?;
    let on_machine = walk(&machine, false)?;
    let old = read_record(song)?;
    let mut record = old.clone();

    for (rel, s_hash) in &store_hashes {
        let rel = *rel;
        let target = machine.join(rel);
        let r_hash = old.get(rel);
        let (state, action) = if is_symlink(&target) {
            (if r_hash.is_some() { State::Edited } else { State::Unrecorded }, Action::Keep)
        } else if let Some(m_path) = on_machine.get(rel) {
            let m_hash = hash_file(m_path)?;
            if &m_hash == s_hash {
                record.insert(rel.clone(), s_hash.clone());
                (State::InSync, Action::None)
            } else {
                match r_hash {
                    Some(r) if *r == m_hash => {
                        record.insert(rel.clone(), s_hash.clone());
                        (State::Stale, Action::Replace)
                    }
                    Some(_) => (State::Edited, Action::Keep),
                    None => (State::Unrecorded, Action::Keep),
                }
            }
        } else if r_hash.is_some() {
            (State::MachineDeleted, Action::Keep)
        } else {
            record.insert(rel.clone(), s_hash.clone());
            (State::New, Action::Add)
        };
        if !check && matches!(action, Action::Replace | Action::Add) {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| io_cause("create", parent, &e).why)?;
            }
            let bytes = std::fs::read(&store[rel]).map_err(|e| io_cause("read", &store[rel], &e).why)?;
            shellbridge::atomic_write_bytes(&target, &bytes).map_err(|e| io_cause("write", &target, &e).why)?;
        }
        report.entries.push(Entry { path: rel.clone(), state, action });
    }

    for (rel, m_path) in &on_machine {
        if store.contains_key(rel) {
            continue;
        }
        match old.get(rel) {
            None => {}
            Some(r) => {
                let unedited = hash_file(m_path)? == *r;
                record.remove(rel);
                let action = if unedited { Action::Delete } else { Action::Keep };
                if !check && unedited {
                    std::fs::remove_file(m_path).map_err(|e| io_cause("remove", m_path, &e).why)?;
                    if let Some(parent) = m_path.parent() {
                        prune_empty_parents(&machine, parent);
                    }
                }
                let state = if unedited { State::Gone } else { State::Edited };
                report.entries.push(Entry { path: rel.clone(), state, action });
            }
        }
    }
    record.retain(|rel, _| store.contains_key(rel) || on_machine.contains_key(rel));
    report.entries.sort_by(|a, b| a.path.cmp(&b.path));

    if !check {
        write_record(song, &source, &record)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, inv, unique_tmp, EnvSaver};

    struct Fx {
        root: PathBuf,
        templates: PathBuf,
        _g: std::sync::MutexGuard<'static, ()>,
        _s: EnvSaver,
    }

    impl Fx {
        fn new(tag: &str) -> Fx {
            let g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
            let s = EnvSaver::capture(&["AOIDE_ROOT", "AOIDE_STAGE_DIR", "AOIDE_SONG_TEMPLATES"]);
            let root = unique_tmp(tag);
            let templates = root.join("templates");
            std::fs::create_dir_all(&templates).unwrap();
            std::env::set_var("AOIDE_ROOT", root.join("aoide"));
            std::env::remove_var("AOIDE_STAGE_DIR");
            std::env::set_var("AOIDE_SONG_TEMPLATES", &templates);
            let fx = Fx { root, templates, _g: g, _s: s };
            fx.builtin(&["sonata"]);
            fx
        }
        fn builtin(&self, songs: &[&str]) {
            let doc = json!({ "declared": "sonata", "songs": songs, "packages": [] });
            std::fs::write(self.templates.join("builtin.json"), doc.to_string()).unwrap();
        }
        fn store(&self, rel: &str, body: &str) {
            put(&self.templates.join("sonata").join(rel), body);
        }
        fn machine(&self, rel: &str, body: &str) {
            put(&shellbridge::songbook_dir("sonata").join(rel), body);
        }
        fn read(&self, rel: &str) -> Option<String> {
            std::fs::read_to_string(shellbridge::songbook_dir("sonata").join(rel)).ok()
        }
        fn record(&self) -> Record {
            read_record("sonata").unwrap()
        }
        fn run(&self, args: &[&str], check: bool) -> Outcome {
            let mut i = inv(&["rice", "refresh"], args);
            if check {
                i.flags.insert("check".into(), "true".into());
            }
            handle_rice_refresh(&i)
        }
    }

    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn put(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn h(body: &str) -> String {
        hex_encode(&sha256(body.as_bytes()))
    }

    fn file_state(out: &Outcome, rel: &str) -> (String, String) {
        let f = out.data.as_ref().unwrap()["songs"][0]["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"] == rel)
            .unwrap_or_else(|| panic!("{rel} not reported: {:?}", out.data))
            .clone();
        (f["state"].as_str().unwrap().into(), f["action"].as_str().unwrap().into())
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
        walk(dir, false).unwrap().into_iter().map(|(k, p)| (k, std::fs::read(p).unwrap())).collect()
    }

    /// Frozen store-shaped fixture: bar.qml, widgets/dock.qml, livery.json.
    fn shipped(fx: &Fx) {
        fx.store("livery.json", "L1");
        fx.store("bar.qml", "B1");
        fx.store("widgets/dock.qml", "D1");
    }

    /// Machine copy equals the store and the record says so.
    fn seeded(fx: &Fx) {
        shipped(fx);
        assert_eq!(fx.run(&[], false).status, Status::Ok);
    }

    #[test]
    fn absent_folder_is_seeded_whole_with_a_record_and_writable_modes() {
        let fx = Fx::new("refresh-absent");
        shipped(&fx);
        for rel in ["livery.json", "bar.qml", "widgets/dock.qml"] {
            std::fs::set_permissions(fx.templates.join("sonata").join(rel), std::fs::Permissions::from_mode(0o444)).unwrap();
        }
        let out = fx.run(&[], false);
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert_eq!(fx.read("widgets/dock.qml").unwrap(), "D1");
        let rec = fx.record();
        assert_eq!(rec.len(), 3);
        assert_eq!(rec["bar.qml"], h("B1"));
        for rel in ["livery.json", "bar.qml", "widgets/dock.qml"] {
            assert_ne!(mode(&shellbridge::songbook_dir("sonata").join(rel)) & 0o200, 0, "{rel}");
        }
        let leftovers: Vec<_> = std::fs::read_dir(shellbridge::songbook_root())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, ["sonata"], "no seed-tmp left behind");
    }

    #[test]
    fn identical_first_run_writes_nothing_but_fills_the_record() {
        let fx = Fx::new("refresh-identical");
        shipped(&fx);
        for rel in ["livery.json", "bar.qml", "widgets/dock.qml"] {
            fx.machine(rel, &std::fs::read_to_string(fx.templates.join("sonata").join(rel)).unwrap());
        }
        let out = fx.run(&[], false);
        assert!(out.changed.is_empty(), "{:?}", out.changed);
        assert_eq!(file_state(&out, "bar.qml"), ("in-sync".into(), "none".into()));
        assert_eq!(fx.record().len(), 3);
    }

    #[test]
    fn stale_file_is_replaced_and_the_record_advances() {
        let fx = Fx::new("refresh-stale");
        seeded(&fx);
        fx.store("bar.qml", "B2");
        let out = fx.run(&[], false);
        assert_eq!(file_state(&out, "bar.qml"), ("stale".into(), "replace".into()));
        assert_eq!(fx.read("bar.qml").unwrap(), "B2");
        assert_eq!(fx.record()["bar.qml"], h("B2"));
    }

    #[test]
    fn edited_file_is_kept_byte_for_byte_and_the_record_does_not_move() {
        let fx = Fx::new("refresh-edited");
        seeded(&fx);
        fx.machine("bar.qml", "MINE");
        fx.store("bar.qml", "B2");
        let out = fx.run(&[], false);
        assert_eq!(file_state(&out, "bar.qml"), ("edited".into(), "keep".into()));
        assert_eq!(fx.read("bar.qml").unwrap(), "MINE");
        assert_eq!(fx.record()["bar.qml"], h("B1"));
        assert_eq!(out.status, Status::Ok);
    }

    #[test]
    fn new_store_file_is_added_and_recorded() {
        let fx = Fx::new("refresh-new");
        seeded(&fx);
        fx.store("widgets/clock.qml", "C1");
        let out = fx.run(&[], false);
        assert_eq!(file_state(&out, "widgets/clock.qml"), ("new".into(), "add".into()));
        assert_eq!(fx.read("widgets/clock.qml").unwrap(), "C1");
        assert_eq!(fx.record()["widgets/clock.qml"], h("C1"));
    }

    #[test]
    fn unrecorded_differing_file_is_kept_and_stays_unrecorded() {
        let fx = Fx::new("refresh-unrecorded");
        shipped(&fx);
        fx.machine("bar.qml", "OLD");
        let out = fx.run(&[], false);
        assert_eq!(file_state(&out, "bar.qml"), ("unrecorded".into(), "keep".into()));
        assert_eq!(fx.read("bar.qml").unwrap(), "OLD");
        assert!(!fx.record().contains_key("bar.qml"));
    }

    #[test]
    fn gone_upstream_unedited_is_deleted_with_its_empty_parent_and_record() {
        let fx = Fx::new("refresh-gone");
        seeded(&fx);
        std::fs::remove_file(fx.templates.join("sonata/widgets/dock.qml")).unwrap();
        std::fs::remove_dir(fx.templates.join("sonata/widgets")).unwrap();
        let out = fx.run(&[], false);
        assert_eq!(file_state(&out, "widgets/dock.qml"), ("gone".into(), "delete".into()));
        assert!(!shellbridge::songbook_dir("sonata").join("widgets").exists());
        assert!(shellbridge::songbook_dir("sonata").is_dir(), "never the song root");
        assert!(!fx.record().contains_key("widgets/dock.qml"));
    }

    #[test]
    fn gone_upstream_but_edited_is_kept_and_dropped_from_the_record() {
        let fx = Fx::new("refresh-gone-edited");
        seeded(&fx);
        fx.machine("bar.qml", "MINE");
        std::fs::remove_file(fx.templates.join("sonata/bar.qml")).unwrap();
        let out = fx.run(&[], false);
        assert_eq!(file_state(&out, "bar.qml"), ("edited".into(), "keep".into()));
        assert_eq!(fx.read("bar.qml").unwrap(), "MINE");
        assert!(!fx.record().contains_key("bar.qml"));
    }

    #[test]
    fn machine_deleted_inherited_file_is_not_re_added() {
        let fx = Fx::new("refresh-machine-deleted");
        seeded(&fx);
        std::fs::remove_file(shellbridge::songbook_dir("sonata").join("bar.qml")).unwrap();
        for _ in 0..2 {
            let out = fx.run(&[], false);
            assert_eq!(file_state(&out, "bar.qml"), ("machine-deleted".into(), "keep".into()));
            assert!(fx.read("bar.qml").is_none());
            assert_eq!(fx.record()["bar.qml"], h("B1"));
        }
    }

    #[test]
    fn runtime_dirs_machine_only_files_and_symlinks_are_never_touched() {
        let fx = Fx::new("refresh-untouched");
        seeded(&fx);
        fx.machine("takes/1/livery.json", "T");
        fx.machine("drafts/d.json", "D");
        fx.machine("notes.md", "mine");
        let link = shellbridge::songbook_dir("sonata").join("link.qml");
        std::os::unix::fs::symlink("/nonexistent", &link).unwrap();
        fx.store("takes/9/x", "shipped takes are not a song file");
        let before = snapshot(&shellbridge::songbook_dir("sonata"));
        let out = fx.run(&[], false);
        assert!(out.changed.is_empty());
        assert_eq!(snapshot(&shellbridge::songbook_dir("sonata")), before);
        assert!(is_symlink(&link));
        assert!(!fx.record().keys().any(|k| k.starts_with("takes") || k == "notes.md"));
        assert!(fx.read("takes/9/x").is_none());
    }

    #[test]
    fn osaka_shape_adds_the_missing_file_and_keeps_the_stale_one_unrecorded() {
        let fx = Fx::new("refresh-osaka");
        fx.store("bar.qml", "NEW");
        fx.store("ricemode.qml", "R");
        fx.machine("bar.qml", "OLD");
        let out = fx.run(&[], false);
        assert_eq!(file_state(&out, "ricemode.qml"), ("new".into(), "add".into()));
        assert_eq!(file_state(&out, "bar.qml"), ("unrecorded".into(), "keep".into()));
        assert_eq!(fx.read("ricemode.qml").unwrap(), "R");
        assert_eq!(fx.read("bar.qml").unwrap(), "OLD");
    }

    #[test]
    fn read_only_tree_has_its_modes_fixed_and_refreshes() {
        let fx = Fx::new("refresh-modes");
        seeded(&fx);
        let song = shellbridge::songbook_dir("sonata");
        fx.store("bar.qml", "B2");
        std::fs::set_permissions(song.join("bar.qml"), std::fs::Permissions::from_mode(0o444)).unwrap();
        std::fs::set_permissions(song.join("widgets/dock.qml"), std::fs::Permissions::from_mode(0o444)).unwrap();
        std::fs::set_permissions(song.join("widgets"), std::fs::Permissions::from_mode(0o555)).unwrap();
        std::fs::set_permissions(&song, std::fs::Permissions::from_mode(0o555)).unwrap();
        let out = fx.run(&[], false);
        assert_eq!(out.status, Status::Ok, "{}", out.message);
        assert_eq!(fx.read("bar.qml").unwrap(), "B2");
        for p in [song.clone(), song.join("widgets"), song.join("widgets/dock.qml")] {
            assert_ne!(mode(&p) & 0o200, 0, "{}", p.display());
        }
    }

    #[test]
    fn symlinked_song_folder_is_skipped() {
        let fx = Fx::new("refresh-symlinked");
        shipped(&fx);
        let elsewhere = fx.root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::create_dir_all(shellbridge::songbook_root()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, shellbridge::songbook_dir("sonata")).unwrap();
        let out = fx.run(&[], false);
        assert_eq!(out.status, Status::Ok);
        assert!(out.data.as_ref().unwrap()["songs"][0]["skipped"].is_string());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        assert!(!shellbridge::songbook_record("sonata").exists());
    }

    #[test]
    fn check_writes_nothing_and_reports_the_same_states() {
        let fx = Fx::new("refresh-check");
        seeded(&fx);
        fx.store("bar.qml", "B2");
        fx.store("widgets/clock.qml", "C1");
        fx.machine("livery.json", "MINE");
        fx.store("livery.json", "L2");
        let song = shellbridge::songbook_dir("sonata");
        let tree = snapshot(&song);
        let record = std::fs::read(shellbridge::songbook_record("sonata")).unwrap();
        let checked = fx.run(&[], true);
        assert_eq!(snapshot(&song), tree);
        assert_eq!(std::fs::read(shellbridge::songbook_record("sonata")).unwrap(), record);
        assert!(checked.changed.is_empty());
        let applied = fx.run(&[], false);
        assert_eq!(checked.data.as_ref().unwrap()["songs"], applied.data.as_ref().unwrap()["songs"]);
        assert_eq!(file_state(&checked, "livery.json"), ("edited".into(), "keep".into()));
    }

    #[test]
    fn check_on_an_absent_folder_seeds_nothing() {
        let fx = Fx::new("refresh-check-absent");
        shipped(&fx);
        let out = fx.run(&[], true);
        assert_eq!(file_state(&out, "bar.qml"), ("new".into(), "add".into()));
        assert!(!shellbridge::songbook_dir("sonata").exists());
        assert!(!shellbridge::songbook_record("sonata").exists());
    }

    #[test]
    fn no_arg_covers_builtin_json_songs_and_an_unknown_name_is_refused() {
        let fx = Fx::new("refresh-names");
        shipped(&fx);
        put(&fx.templates.join("other/livery.json"), "O");
        let out = fx.run(&[], false);
        assert_eq!(out.data.as_ref().unwrap()["songs"].as_array().unwrap().len(), 1);
        assert!(!shellbridge::songbook_dir("other").exists(), "not in builtin.json");
        let named = fx.run(&["other"], false);
        assert_eq!(named.status, Status::Ok);
        assert!(shellbridge::songbook_dir("other").is_dir());
        let unknown = fx.run(&["nope"], false);
        assert_eq!(unknown.status, Status::Error);
        assert!(unknown.message.contains("`nope`"));
        let traversal = fx.run(&["../x"], false);
        assert_eq!(traversal.status, Status::Error);
    }

    #[test]
    fn a_refreshed_unedited_song_equals_the_store() {
        let fx = Fx::new("refresh-equal");
        seeded(&fx);
        fx.machine("takes/1/x", "scratch");
        fx.store("bar.qml", "B2");
        fx.store("widgets/clock.qml", "C1");
        std::fs::remove_file(fx.templates.join("sonata/livery.json")).unwrap();
        fx.run(&[], false);
        let store = fx.templates.join("sonata");
        assert!(crate::widgets::trees_equal(
            &shellbridge::songbook_dir("sonata"),
            &store,
            MACHINE_RUNTIME_DIRS
        ));
    }

    #[test]
    fn second_run_is_a_no_op() {
        let fx = Fx::new("refresh-idempotent");
        seeded(&fx);
        fx.store("bar.qml", "B2");
        assert!(!fx.run(&[], false).changed.is_empty());
        let song = shellbridge::songbook_dir("sonata");
        let tree = snapshot(&song);
        let record = std::fs::read(shellbridge::songbook_record("sonata")).unwrap();
        let again = fx.run(&[], false);
        assert!(again.changed.is_empty());
        assert_eq!(snapshot(&song), tree);
        assert_eq!(std::fs::read(shellbridge::songbook_record("sonata")).unwrap(), record);
    }

    #[test]
    fn a_symlink_in_the_shipped_copy_is_a_loud_error() {
        let fx = Fx::new("refresh-store-symlink");
        shipped(&fx);
        std::os::unix::fs::symlink("/etc/hostname", fx.templates.join("sonata/evil.qml")).unwrap();
        let out = fx.run(&[], false);
        assert_eq!(out.status, Status::Error);
        assert!(out.message.contains("symlink"), "{}", out.message);
        assert!(!shellbridge::songbook_dir("sonata").exists());
    }

    #[test]
    fn the_stage_time_seed_writes_the_record_too() {
        let fx = Fx::new("refresh-stage-seed");
        shipped(&fx);
        let seeded = crate::commands::rice::seed_songbook_from_templates("sonata").unwrap();
        assert_eq!(seeded, Some(fx.templates.join("sonata")));
        assert_eq!(fx.record().len(), 3);
        assert_eq!(fx.read("bar.qml").unwrap(), "B1");
        assert!(crate::commands::rice::seed_songbook_from_templates("sonata").unwrap().is_none());
    }
}
