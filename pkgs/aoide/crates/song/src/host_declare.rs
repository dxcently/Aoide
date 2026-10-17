//! `lyra rice declare <song> --host` — make `<song>` the one this host
//! performs on its next rebuild, by rewriting the host config's
//! `habit.song.declared = "<name>";` line (CONTRACTS.md §3, the rice commands).
//!
//! The host file is [`aoide_storage::fs::host_config`] (`aoide.hostConfig`).
//! The line must occur EXACTLY once, in the shape nixfmt writes it: leading
//! indentation, blanks around `=` and before `;`, nothing after the `;`. Zero
//! or several such lines is a refusal naming the file and the count; the
//! command never guesses which line a host means.
//!
//! A song can be declared only when the songbook the host builds from carries
//! it: the build's built-in set, `builtin.json`'s `songs` in the shipped
//! templates (`declared ∪ available`, closed under borrows). A song that is
//! only in the machine's runtime songbook is refused, and so is a staged
//! draft: a draft is not a song.
//!
//! Only the bytes between the quotes change, written to a temp file in the
//! file's own directory and renamed over it, with the file's mode kept. No
//! git, no rebuild (house rule 2): the answer says it takes effect on the next
//! rebuild, and `stagedDiffers` says when the machine copy of the song carries
//! livery or widget edits the build's copy lacks, since the rebuild builds the
//! songbook version.

use crate::commands::refresh::staged_differs;
use crate::widgets::read_builtin;
use aoide_protocol::output::{io_cause, Fix, Kind, Outcome};
use aoide_storage::fs as shellbridge;
use aoide_storage::mode::{load_mode_marker, RiceMode};
use serde_json::{json, Value};
use std::ops::Range;
use std::path::{Path, PathBuf};

const CMD: &str = "rice.declare";
const KEY: &str = "habit.song.declared";

/// Run the declare against this machine's host file and shipped templates.
pub fn declare_on_host(name: &str) -> Outcome {
    declare(name, shellbridge::host_config(), shellbridge::song_templates_dir())
}

fn refuse(kind: Kind, reason: &str, what: String, why: String, fix: Fix) -> Outcome {
    Outcome::refuse(CMD, kind, what, why, fix).with_fields(json!({ "reason": reason }))
}

fn declare(name: &str, host_file: Option<PathBuf>, templates: Option<PathBuf>) -> Outcome {
    if !crate::compose::valid_song_name(name) {
        return refuse(
            Kind::Usage,
            "invalid-name",
            format!("`{name}` is not a song name"),
            "a song name is lowercase letters, digits and hyphens, with no leading hyphen".into(),
            Fix::Run("lyra rice list".into()),
        );
    }

    let marker = load_mode_marker();
    if marker.mode == RiceMode::Draft
        && (marker.song.as_deref() == Some(name) || marker.draft.as_deref() == Some(name))
    {
        let draft = marker.draft.unwrap_or_default();
        return refuse(
            Kind::Refused,
            "staged-draft",
            format!("the stage holds draft `{draft}`, not the song `{name}`"),
            "only a song can be declared, and a draft lives in the machine's songbook alone".into(),
            Fix::Run("lyra rice mode stage".into()),
        );
    }

    let Some(file) = host_file else {
        return refuse(
            Kind::Refused,
            "no-host-config",
            "no host config file is known".into(),
            "`$AOIDE_HOST_CONFIG` is unset and this machine's hostname could not be read".into(),
            Fix::Set("aoide.hostConfig".into()),
        );
    };
    let file = std::fs::canonicalize(&file).unwrap_or(file);
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) => {
            let cause = io_cause("read", &file, &e);
            return refuse(
                Kind::Refused,
                "host-config-unreadable",
                format!("cannot read the host config `{}`", file.display()),
                cause.why,
                Fix::Set("aoide.hostConfig".into()),
            )
            .with_fields(json!({ "detail": cause.detail }));
        }
    };

    let lines = declared_values(&text);
    if lines.len() != 1 {
        return refuse(
            Kind::Refused,
            "host-line-count",
            format!(
                "`{}` holds {} `{KEY} = \"…\";` lines, not one",
                file.display(),
                lines.len()
            ),
            "the declare rewrites exactly one line and never guesses which one a host means".into(),
            Fix::Set(format!("exactly one `{KEY} = \"<song>\";` line in `{}`", file.display())),
        )
        .with_fields(json!({ "hostConfig": file.to_string_lossy(), "count": lines.len() }));
    }
    let range = lines[0].clone();
    let previous = text[range.clone()].to_string();
    let host = aoide_storage::display::os_hostname().unwrap_or_else(|| "this host".into());

    let differs: Value = match templates.as_deref() {
        Some(t) => staged_differs(name, t).map(Value::Bool).unwrap_or(Value::Null),
        None => Value::Null,
    };
    let data = |changed: bool| {
        json!({
            "name": name,
            "host": host,
            "hostConfig": file.to_string_lossy(),
            "previous": previous,
            "changed": changed,
            "stagedDiffers": differs,
        })
    };
    let caveat = if differs == Value::Bool(true) {
        " The staged copy has edits the build's copy lacks: the rebuild builds the songbook version, not the live edits."
    } else {
        ""
    };

    if previous == name {
        return Outcome::ok(
            CMD,
            format!("`{name}` is already {host}'s declared song in {}.{caveat}", file.display()),
        )
        .gated(true)
        .with_data(data(false));
    }

    let built_in = match templates.as_deref().map(read_builtin) {
        Some(Ok(b)) => b.songs,
        Some(Err(e)) => {
            return refuse(
                Kind::Failed,
                "builtin-unreadable",
                "cannot read the songs this host builds".into(),
                e.error,
                Fix::None("the shipped templates are broken; a rebuild replaces them"),
            )
        }
        None => Vec::new(),
    };
    if !built_in.iter().any(|s| s == name) {
        let carried = if built_in.is_empty() { "none".to_string() } else { built_in.join(", ") };
        return refuse(
            Kind::Refused,
            "not-built-in",
            format!("`{name}` is not in the songbook {host} builds from"),
            format!(
                "this build carries {carried}; a song staged only from the machine's songbook does not exist for the rebuild"
            ),
            Fix::Set(format!(
                "`habit.song.available` to include \"{name}\" in `{}`, rebuild, then declare",
                file.display()
            )),
        )
        .with_fields(json!({ "builtIn": built_in }));
    }

    let mut rewritten = String::with_capacity(text.len() + name.len());
    rewritten.push_str(&text[..range.start]);
    rewritten.push_str(name);
    rewritten.push_str(&text[range.end..]);
    if let Err(e) = write_keeping_mode(&file, rewritten.as_bytes()) {
        let cause = io_cause("write", &file, &e);
        return refuse(
            Kind::Failed,
            "write-failed",
            format!("cannot rewrite `{}`", file.display()),
            cause.why,
            Fix::Set(format!("write permission on `{}`'s directory", file.display())),
        )
        .with_fields(json!({ "detail": cause.detail }));
    }

    Outcome::ok(
        CMD,
        format!(
            "declared `{name}` as {host}'s song in {} (was `{previous}`); takes effect on the next rebuild.{caveat}",
            file.display()
        ),
    )
    .gated(true)
    .changed([file.to_string_lossy().into_owned()])
    .with_data(data(true))
}

/// The byte range of the quoted value on every line that is exactly one
/// `habit.song.declared = "<value>";` statement, whitespace aside.
fn declared_values(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if let Some(r) = value_in(line) {
            out.push(at + r.start..at + r.end);
        }
        at += line.len();
    }
    out
}

fn value_in(line: &str) -> Option<Range<usize>> {
    let blank = |c: char| c == ' ' || c == '\t';
    let body = line.trim_end_matches(['\n', '\r']);
    let rest = body.trim_start_matches(blank).strip_prefix(KEY)?;
    let inner = rest.trim_start_matches(blank).strip_prefix('=')?.trim_start_matches(blank).strip_prefix('"')?;
    let close = inner.find('"')?;
    if inner[..close].contains(['\\', '$']) {
        return None;
    }
    let tail = inner[close + 1..].trim_start_matches(blank).strip_prefix(';')?;
    if !tail.trim_matches(blank).is_empty() {
        return None;
    }
    let start = body.len() - inner.len();
    Some(start..start + close)
}

/// Write `bytes` to a temp file beside `path`, give it `path`'s mode, and
/// rename it over `path`.
fn write_keeping_mode(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mode = std::fs::metadata(path)?.permissions();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.aoide-declare.{}", std::process::id()));
    let written = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::set_permissions(&tmp, mode)?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::output::Status;
    use aoide_test_support::{env_lock, unique_tmp, EnvSaver};
    use std::os::unix::fs::PermissionsExt;

    struct Rig {
        root: PathBuf,
        host: PathBuf,
        templates: PathBuf,
        _env: EnvSaver,
    }

    /// A scratch runtime root, a host file holding `host_text`, and shipped
    /// templates whose `builtin.json` builds `built_in`, each song shipped with
    /// a `livery.json` and one widget.
    fn rig(tag: &str, host_text: &str, built_in: &[&str]) -> Rig {
        let env = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT"]);
        let root = unique_tmp(tag);
        std::env::set_var("AOIDE_STAGE_DIR", root.join("song").join("stage"));
        std::fs::create_dir_all(root.join("song").join("stage")).unwrap();
        let host = root.join("hosts").join("box").join("default.nix");
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(&host, host_text).unwrap();
        let templates = root.join("templates");
        for song in built_in {
            let dir = templates.join(song).join("widgets");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(templates.join(song).join("livery.json"), format!("{{\"song\":\"{song}\"}}")).unwrap();
            std::fs::write(dir.join("bar.qml"), "Item {}\n").unwrap();
        }
        std::fs::create_dir_all(&templates).unwrap();
        std::fs::write(
            templates.join("builtin.json"),
            json!({ "declared": "sonata", "songs": built_in, "packages": [] }).to_string(),
        )
        .unwrap();
        Rig { root, host, templates, _env: env }
    }

    impl Rig {
        fn run(&self, name: &str) -> Outcome {
            declare(name, Some(self.host.clone()), Some(self.templates.clone()))
        }
        fn host_text(&self) -> String {
            std::fs::read_to_string(&self.host).unwrap()
        }
        fn reason(out: &Outcome) -> String {
            out.data.as_ref().and_then(|d| d["reason"].as_str()).unwrap_or("").to_string()
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const HOST: &str = "{ ... }:\n{\n  habit.song.declared = \"sonata\";\n  habit.song.available = [ ];\n}\n";

    #[test]
    fn one_line_is_rewritten_between_the_quotes_only() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-one", HOST, &["sonata", "barcarolle"]);
        let out = r.run("barcarolle");
        assert_eq!(out.status, Status::Ok, "{out:?}");
        assert_eq!(r.host_text(), HOST.replace("\"sonata\"", "\"barcarolle\""));
        let d = out.data.unwrap();
        assert_eq!(d["previous"], "sonata");
        assert_eq!(d["changed"], true);
        assert_eq!(d["stagedDiffers"], false, "no machine copy differs in nothing");
        assert!(out.message.contains("takes effect on the next rebuild"), "{}", out.message);
        assert_eq!(out.changed.len(), 1);
    }

    #[test]
    fn no_line_is_refused_naming_the_file_and_the_count() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let text = "{\n  # habit.song.declared = \"sonata\";\n  habit.song = { declared = \"sonata\"; };\n}\n";
        let r = rig("hd-zero", text, &["sonata", "barcarolle"]);
        let out = r.run("barcarolle");
        assert_eq!(out.status, Status::Error);
        assert_eq!(Rig::reason(&out), "host-line-count");
        assert_eq!(out.data.as_ref().unwrap()["count"], 0);
        assert!(out.message.contains(&r.host.display().to_string()), "{}", out.message);
        assert_eq!(r.host_text(), text, "a refusal writes nothing");
    }

    #[test]
    fn two_lines_are_refused_never_guessed() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let text = "{\n  habit.song.declared = \"sonata\";\n  habit.song.declared = \"solo\";\n}\n";
        let r = rig("hd-two", text, &["sonata", "barcarolle"]);
        let out = r.run("barcarolle");
        assert_eq!(Rig::reason(&out), "host-line-count");
        assert_eq!(out.data.as_ref().unwrap()["count"], 2);
        assert_eq!(r.host_text(), text);
    }

    #[test]
    fn nixfmt_spacing_variants_match_and_anything_else_does_not() {
        for line in [
            "habit.song.declared = \"sonata\";",
            "    habit.song.declared = \"sonata\";",
            "\thabit.song.declared=\"sonata\";",
            "habit.song.declared  =  \"sonata\" ;  ",
            "habit.song.declared = \"sonata\";\r",
        ] {
            let r = value_in(line).unwrap_or_else(|| panic!("should match: {line:?}"));
            assert_eq!(&line[r], "sonata", "{line:?}");
        }
        for line in [
            "# habit.song.declared = \"sonata\";",
            "habit.song.declared = \"sonata\"; # performs",
            "habit.song.declared = \"son${x}ata\";",
            "habit.song.declared = lib.mkDefault \"sonata\";",
            "habit.song.declaredX = \"sonata\";",
            "habit.song.declared = \"sonata\"",
            "habit.song.available = [ \"sonata\" ];",
        ] {
            assert!(value_in(line).is_none(), "should not match: {line:?}");
        }
    }

    #[test]
    fn an_unknown_song_is_refused_with_the_built_in_set() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-unknown", HOST, &["sonata"]);
        std::fs::create_dir_all(r.root.join("song").join("songbook").join("fugue")).unwrap();
        let out = r.run("fugue");
        assert_eq!(out.status, Status::Error);
        assert_eq!(Rig::reason(&out), "not-built-in");
        assert_eq!(out.data.as_ref().unwrap()["builtIn"], json!(["sonata"]));
        assert_eq!(r.host_text(), HOST);
    }

    #[test]
    fn a_staged_draft_is_refused() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-draft", HOST, &["sonata", "barcarolle"]);
        std::fs::write(
            aoide_storage::mode::mode_marker_path(),
            r#"{"mode":"draft","song":"barcarolle","draft":"draft-2"}"#,
        )
        .unwrap();
        for name in ["barcarolle", "draft-2"] {
            let out = r.run(name);
            assert_eq!(out.status, Status::Error, "{name}");
            assert_eq!(Rig::reason(&out), "staged-draft", "{name}");
        }
        assert_eq!(r.host_text(), HOST);
    }

    #[test]
    fn already_declared_is_a_no_op_success() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-same", HOST, &["sonata"]);
        let before = std::fs::metadata(&r.host).unwrap().modified().unwrap();
        let out = r.run("sonata");
        assert_eq!(out.status, Status::Ok, "{out:?}");
        assert_eq!(out.data.as_ref().unwrap()["changed"], false);
        assert!(out.message.contains("already"), "{}", out.message);
        assert!(out.changed.is_empty());
        assert_eq!(std::fs::metadata(&r.host).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn the_files_mode_is_kept() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-mode", HOST, &["sonata", "barcarolle"]);
        std::fs::set_permissions(&r.host, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert_eq!(r.run("barcarolle").status, Status::Ok);
        assert_eq!(std::fs::metadata(&r.host).unwrap().permissions().mode() & 0o777, 0o640);
    }

    #[test]
    fn the_write_is_a_rename_and_leaves_no_temp() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-atomic", HOST, &["sonata", "barcarolle"]);
        use std::os::unix::fs::MetadataExt;
        let before = std::fs::metadata(&r.host).unwrap().ino();
        assert_eq!(r.run("barcarolle").status, Status::Ok);
        assert_ne!(std::fs::metadata(&r.host).unwrap().ino(), before, "a new inode: renamed in, not written in place");
        let left: Vec<_> = std::fs::read_dir(r.host.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["default.nix"]);
    }

    #[test]
    fn a_failed_write_leaves_the_file_and_no_temp() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-ro", HOST, &["sonata", "barcarolle"]);
        let dir = r.host.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let out = r.run("barcarolle");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        if out.status == Status::Ok {
            return; // running as root: the directory's mode binds nobody
        }
        assert_eq!(Rig::reason(&out), "write-failed");
        assert_eq!(r.host_text(), HOST);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[test]
    fn staged_edits_are_reported() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-differs", HOST, &["sonata", "barcarolle"]);
        let machine = shellbridge::songbook_dir("barcarolle");
        shellbridge::copy_dir_recursive(&r.templates.join("barcarolle"), &machine).unwrap();
        std::fs::create_dir_all(machine.join("takes")).unwrap();
        std::fs::write(machine.join("takes").join("head.json"), "{}").unwrap();
        std::fs::write(machine.join("notes.md"), "machine-only prose").unwrap();
        let same = r.run("barcarolle");
        assert_eq!(same.data.as_ref().unwrap()["stagedDiffers"], false, "takes/ and prose are not the dress");

        std::fs::write(machine.join("widgets").join("bar.qml"), "Item { width: 3 }\n").unwrap();
        let out = r.run("barcarolle");
        assert_eq!(out.status, Status::Ok);
        assert_eq!(out.data.as_ref().unwrap()["stagedDiffers"], true);
        assert!(out.message.contains("not the live edits"), "{}", out.message);
    }

    #[test]
    fn an_invalid_name_is_usage_and_a_missing_file_is_refused() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-shape", HOST, &["sonata"]);
        assert_eq!(r.run("../evil").status, Status::Usage);
        let out = declare("sonata", Some(r.root.join("nope.nix")), Some(r.templates.clone()));
        assert_eq!(Rig::reason(&out), "host-config-unreadable");
        assert_eq!(Rig::reason(&declare("sonata", None, None)), "no-host-config");
    }
}
