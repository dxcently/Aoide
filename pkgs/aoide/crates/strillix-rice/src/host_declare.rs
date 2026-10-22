//! `strillix rice declare <rice> --host` — make `<rice>` the one this host
//! performs on its next rebuild, by rewriting the host config's
//! `habit.rice.declared = "<name>";` line (CONTRACTS.md §3, the rice commands).
//!
//! The host file is [`crate::paths::host_config`] (`strillix.hostConfig`).
//! The line must occur EXACTLY once, in the shape nixfmt writes it: leading
//! indentation, blanks around `=` and before `;`, nothing after the `;`. Zero
//! or several such lines is a refusal naming the file and the count; the
//! command never guesses which line a host means. The matcher is line-based
//! and parses no Nix: it only skips lines that start inside a `/* … */`
//! comment or a `''…''` / `"…"` string, so a dead line counts as no line.
//! It cannot see an override set elsewhere (an `mkForce` in another module),
//! so what the next rebuild performs is the rebuild's answer, not this one's.
//!
//! A rice can be declared only when the rices the host builds from carries
//! it as a performer: the build's built-in set, `builtin.json`'s `rices` in
//! the shipped templates (`declared ∪ available`, closed under borrows), with
//! the rice's own `rice.nix` shipped beside it (`lib/rices.nix` refuses to
//! declare a rice without one, so a lending-only rice is refused here first).
//! A rice that is only in the machine's runtime rices is refused, and so is
//! a staged draft: a draft is not a rice.
//!
//! Only the bytes between the quotes change, written to a temp file in the
//! file's own directory, created with the file's mode, and renamed over it,
//! the directory synced after. A file whose size, mtime or inode moved between
//! the read and the final re-stat is refused (`host-config-changed`), not
//! overwritten. That narrows the window for losing someone else's edit to the
//! microseconds between the re-stat and the rename; it does not close it. No
//! git, no rebuild (house rule 2): the answer says it takes effect on the next
//! rebuild, and `stagedDiffers` says when the machine copy of the rice carries
//! theme or widget edits the build's copy lacks, since the rebuild builds the
//! rices version.

use crate::checkout_gate;
use crate::commands::refresh::staged_differs;
use crate::widgets::read_builtin;
use aoide_protocol::output::{io_cause, Fix, Kind, Outcome, Status};
use aoide_storage::fs as shellbridge;
use crate::mode::{load_mode_marker, RiceMode};
use serde_json::{json, Value};
use std::ops::Range;
use std::path::{Path, PathBuf};

const CMD: &str = "rice.declare";
const KEY: &str = "habit.rice.declared";

/// Run the declare against this machine's host file and shipped templates. A
/// declared rice whose checkout folder holds uncommitted changes is still
/// declared (this path never refuses for git), with a warning field: the
/// rebuild builds the committed rices, so those edits are not in it.
pub fn declare_on_host(name: &str) -> Outcome {
    let out = declare(name, crate::paths::host_config(), crate::paths::rice_templates_dir());
    if out.status != Status::Ok {
        return out;
    }
    let folder = format!("rices/{name}");
    let dirty = checkout_gate::dirty_lines(&shellbridge::flake_root(), &folder);
    if dirty.is_empty() {
        out
    } else {
        out.with_fields(json!({
            "warning": format!(
                "`{folder}` has uncommitted changes; the rebuild builds the committed rices, not these edits"
            ),
            "uncommitted": dirty,
        }))
    }
}

fn refuse(kind: Kind, reason: &str, what: String, why: String, fix: Fix) -> Outcome {
    Outcome::refuse(CMD, kind, what, why, fix).with_fields(json!({ "reason": reason }))
}

fn declare(name: &str, host_file: Option<PathBuf>, templates: Option<PathBuf>) -> Outcome {
    declare_racing(name, host_file, templates, &|| {})
}

/// [`declare`], with `before_rename` run between the read and the re-stat:
/// the seam the tests edit the file through.
fn declare_racing(
    name: &str,
    host_file: Option<PathBuf>,
    templates: Option<PathBuf>,
    before_rename: &dyn Fn(),
) -> Outcome {
    if !crate::compose::valid_rice_name(name) {
        return refuse(
            Kind::Usage,
            "invalid-name",
            format!("`{name}` is not a rice name"),
            "a rice name is lowercase letters, digits and hyphens, with no leading hyphen".into(),
            Fix::Run("strillix rice list".into()),
        );
    }

    let marker = load_mode_marker();
    if marker.mode == RiceMode::Draft
        && (marker.rice.as_deref() == Some(name) || marker.draft.as_deref() == Some(name))
    {
        let draft = marker.draft.unwrap_or_default();
        return refuse(
            Kind::Refused,
            "staged-draft",
            format!("the stage holds draft `{draft}`, not the rice `{name}`"),
            "only a rice can be declared, and a draft lives in the machine's rices alone".into(),
            Fix::Run("strillix rice mode stage".into()),
        );
    }

    let Some(file) = host_file else {
        return refuse(
            Kind::Refused,
            "no-host-config",
            "no host config file is known".into(),
            "`$STRILLIX_HOST_CONFIG` is unset and this machine's hostname could not be read".into(),
            Fix::Set("strillix.hostConfig".into()),
        );
    };
    let file = std::fs::canonicalize(&file).unwrap_or(file);
    let read = std::fs::metadata(&file).and_then(|m| Ok((Stamp::of(&m), std::fs::read_to_string(&file)?)));
    let (seen, text) = match read {
        Ok(read) => read,
        Err(e) => {
            let cause = io_cause("read", &file, &e);
            return refuse(
                Kind::Refused,
                "host-config-unreadable",
                format!("cannot read the host config `{}`", file.display()),
                cause.why,
                Fix::Set("strillix.hostConfig".into()),
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
            Fix::Set(format!("exactly one `{KEY} = \"<rice>\";` line in `{}`", file.display())),
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
        " The staged copy has edits the build's copy lacks: the rebuild builds the rices version, not the live edits."
    } else {
        ""
    };

    if previous == name {
        return Outcome::ok(
            CMD,
            format!("`{name}` is already {host}'s declared rice in {}.{caveat}", file.display()),
        )
        .gated(true)
        .with_data(data(false));
    }

    let built_in = match templates.as_deref().map(read_builtin) {
        Some(Ok(b)) => b.rices,
        Some(Err(e)) => {
            return refuse(
                Kind::Failed,
                "builtin-unreadable",
                "cannot read the rices this host builds".into(),
                e.error,
                Fix::None("the shipped templates are broken; a rebuild replaces them"),
            )
        }
        None => Vec::new(),
    };
    if !built_in.iter().any(|s| s == name) {
        let carried = match templates {
            None => "the build's rice list is unknown (no shipped templates)".to_string(),
            Some(_) if built_in.is_empty() => "this build carries no rice".to_string(),
            Some(_) => format!("this build carries {}", built_in.join(", ")),
        };
        return refuse(
            Kind::Refused,
            "not-built-in",
            format!("`{name}` is not in the rices {host} builds from"),
            format!("{carried}; a rice staged only from the machine's rices does not exist for the rebuild"),
            Fix::Set(format!(
                "`habit.rice.available` to include \"{name}\" in `{}`, rebuild, then declare",
                file.display()
            )),
        )
        .with_fields(json!({ "builtIn": built_in }));
    }
    let shipped = templates.as_deref().map(|t| t.join(name).join("rice.nix"));
    if !shipped.as_deref().is_some_and(Path::is_file) {
        return refuse(
            Kind::Refused,
            "no-rice-nix",
            format!("`{name}` ships no rice.nix in this build"),
            "the build carries it only as a lender of widgets; a declared rice needs its own rice.nix to perform".into(),
            Fix::Set(format!("a `rices/{name}/rice.nix`, rebuild, then declare")),
        )
        .with_fields(json!({ "builtIn": built_in }));
    }

    let mut rewritten = String::with_capacity(text.len() + name.len());
    rewritten.push_str(&text[..range.start]);
    rewritten.push_str(name);
    rewritten.push_str(&text[range.end..]);
    if let Err(e) = write_keeping_mode(&file, rewritten.as_bytes(), &seen, before_rename) {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            return refuse(
                Kind::Refused,
                "host-config-changed",
                format!("`{}` was edited while declaring", file.display()),
                "the file moved between the read and the write, and that edit is kept, not overwritten; try again".into(),
                Fix::Run(format!("strillix rice declare {name} --host")),
            )
            .with_fields(json!({ "hostConfig": file.to_string_lossy() }));
        }
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
            "declared `{name}` as {host}'s rice in {} (was `{previous}`); takes effect on the next rebuild.{caveat}",
            file.display()
        ),
    )
    .gated(true)
    .changed([file.to_string_lossy().into_owned()])
    .with_data(data(true))
}

/// The rice a host file declares: the value of its one live
/// `habit.rice.declared` line. `None` when the file cannot be read or holds
/// zero or several such lines, the counts `rice declare` refuses on.
pub(crate) fn declared_in_file(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    match declared_values(&text).as_slice() {
        [only] => Some(text[only.clone()].to_string()),
        _ => None,
    }
}

/// The byte range of the quoted value on every live line that is exactly one
/// `habit.rice.declared = "<value>";` statement, whitespace aside. A line is
/// live when it starts outside a comment or string ([`Lexer`]).
fn declared_values(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut lexer = Lexer::Code;
    for line in text.split_inclusive('\n') {
        if lexer == Lexer::Code {
            if let Some(r) = value_in(line) {
                out.push(at + r.start..at + r.end);
            }
        }
        lexer = lexer.through(line);
        at += line.len();
    }
    out
}

/// Where a line ends, lexically: just enough of Nix to tell code from a
/// `/* … */` comment, a `''…''` string or a `"…"` string that spans lines.
/// Not a parser: `${ … }` inside a string is read as string.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Lexer {
    Code,
    Block,
    Indented,
    Quoted,
}

impl Lexer {
    fn through(mut self, line: &str) -> Lexer {
        let b = line.as_bytes();
        let at = |i: usize| b.get(i).copied();
        let mut i = 0;
        while i < b.len() {
            match self {
                Lexer::Code => match (b[i], at(i + 1)) {
                    (b'#', _) => return self,
                    (b'/', Some(b'*')) => (self, i) = (Lexer::Block, i + 1),
                    (b'\'', Some(b'\'')) => (self, i) = (Lexer::Indented, i + 1),
                    (b'"', _) => self = Lexer::Quoted,
                    _ => {}
                },
                Lexer::Block => {
                    if (b[i], at(i + 1)) == (b'*', Some(b'/')) {
                        (self, i) = (Lexer::Code, i + 1);
                    }
                }
                Lexer::Indented => match (b[i], at(i + 1), at(i + 2)) {
                    (b'\'', Some(b'\''), Some(b'\'' | b'$')) => i += 2,
                    (b'\'', Some(b'\''), Some(b'\\')) => i += 3,
                    (b'\'', Some(b'\''), _) => (self, i) = (Lexer::Code, i + 1),
                    _ => {}
                },
                Lexer::Quoted => match b[i] {
                    b'\\' => i += 1,
                    b'"' => self = Lexer::Code,
                    _ => {}
                },
            }
            i += 1;
        }
        self
    }
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

/// What the read saw of the host file: a later stat that differs means
/// someone else wrote it in between.
#[derive(PartialEq)]
struct Stamp {
    len: u64,
    mtime: Option<std::time::SystemTime>,
    ino: u64,
}

impl Stamp {
    fn of(m: &std::fs::Metadata) -> Stamp {
        use std::os::unix::fs::MetadataExt;
        Stamp { len: m.len(), mtime: m.modified().ok(), ino: m.ino() }
    }
}

/// Create `tmp` carrying `mode` from the open on, so a 0600 file is never
/// readable under its temp name. The umask may narrow it; the caller sets the
/// exact mode after.
fn create_temp(tmp: &Path, mode: &std::fs::Permissions) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::OpenOptions::new().write(true).create_new(true).mode(mode.mode() & 0o7777).open(tmp)
}

/// Write `bytes` to a temp file beside `path`, created with `path`'s mode, and
/// rename it over `path`, then sync the directory so the rename is durable.
/// `path` re-stated as anything but `seen` just before the rename is a
/// `WouldBlock` error and nothing is renamed. A failed directory sync after
/// the rename is not an error: the new file is already in place.
fn write_keeping_mode(path: &Path, bytes: &[u8], seen: &Stamp, before_rename: &dyn Fn()) -> std::io::Result<()> {
    use std::io::Write;
    let mode = std::fs::metadata(path)?.permissions();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.aoide-declare.{}", std::process::id()));
    let written = (|| {
        let mut f = create_temp(&tmp, &mode)?;
        f.set_permissions(mode)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        before_rename();
        if Stamp::of(&std::fs::metadata(path)?) != *seen {
            return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "host config changed"));
        }
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return written;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
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
    /// templates whose `builtin.json` builds `built_in`, each rice shipped with
    /// a `theme.json` and one widget.
    fn rig(tag: &str, host_text: &str, built_in: &[&str]) -> Rig {
        let env = EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let root = unique_tmp(tag);
        aoide_test_support::pin_stage(root.join("rice").join("stage"));
        std::fs::create_dir_all(root.join("rice").join("stage")).unwrap();
        let host = root.join("hosts").join("box").join("default.nix");
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(&host, host_text).unwrap();
        let templates = root.join("templates");
        for rice in built_in {
            let dir = templates.join(rice).join("widgets");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(templates.join(rice).join("theme.json"), format!("{{\"rice\":\"{rice}\"}}")).unwrap();
            std::fs::write(dir.join("bar.qml"), "Item {}\n").unwrap();
            std::fs::write(templates.join(rice).join("rice.nix"), "{ ... }: { }\n").unwrap();
        }
        std::fs::create_dir_all(&templates).unwrap();
        std::fs::write(
            templates.join("builtin.json"),
            json!({ "declared": "sonata", "rices": built_in, "packages": [] }).to_string(),
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

    const HOST: &str = "{ ... }:\n{\n  habit.rice.declared = \"sonata\";\n  habit.rice.available = [ ];\n}\n";

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
        let text = "{\n  # habit.rice.declared = \"sonata\";\n  habit.rice = { declared = \"sonata\"; };\n}\n";
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
        let text = "{\n  habit.rice.declared = \"sonata\";\n  habit.rice.declared = \"solo\";\n}\n";
        let r = rig("hd-two", text, &["sonata", "barcarolle"]);
        let out = r.run("barcarolle");
        assert_eq!(Rig::reason(&out), "host-line-count");
        assert_eq!(out.data.as_ref().unwrap()["count"], 2);
        assert_eq!(r.host_text(), text);
    }

    #[test]
    fn nixfmt_spacing_variants_match_and_anything_else_does_not() {
        for line in [
            "habit.rice.declared = \"sonata\";",
            "    habit.rice.declared = \"sonata\";",
            "\thabit.rice.declared=\"sonata\";",
            "habit.rice.declared  =  \"sonata\" ;  ",
            "habit.rice.declared = \"sonata\";\r",
        ] {
            let r = value_in(line).unwrap_or_else(|| panic!("should match: {line:?}"));
            assert_eq!(&line[r], "sonata", "{line:?}");
        }
        for line in [
            "# habit.rice.declared = \"sonata\";",
            "habit.rice.declared = \"sonata\"; # performs",
            "habit.rice.declared = \"son${x}ata\";",
            "habit.rice.declared = lib.mkDefault \"sonata\";",
            "habit.rice.declaredX = \"sonata\";",
            "habit.rice.declared = \"sonata\"",
            "habit.rice.available = [ \"sonata\" ];",
        ] {
            assert!(value_in(line).is_none(), "should not match: {line:?}");
        }
    }

    #[test]
    fn an_unknown_rice_is_refused_with_the_built_in_set() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-unknown", HOST, &["sonata"]);
        std::fs::create_dir_all(r.root.join("rice").join("rices").join("fugue")).unwrap();
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
            crate::mode::mode_marker_path(),
            r#"{"mode":"draft","rice":"barcarolle","draft":"draft-2"}"#,
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
    fn a_0600_file_is_never_wider_under_its_temp_name() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-0600", HOST, &["sonata", "barcarolle"]);
        std::fs::set_permissions(&r.host, std::fs::Permissions::from_mode(0o600)).unwrap();
        let dir = r.host.parent().unwrap().to_path_buf();
        let seen = std::cell::Cell::new(None);
        let out = declare_racing("barcarolle", Some(r.host.clone()), Some(r.templates.clone()), &|| {
            let tmp = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).find(|p| p != &r.host).unwrap();
            seen.set(Some(std::fs::metadata(tmp).unwrap().permissions().mode() & 0o777));
        });
        assert_eq!(out.status, Status::Ok, "{out:?}");
        assert_eq!(seen.get(), Some(0o600), "the temp before the rename");
        assert_eq!(std::fs::metadata(&r.host).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn the_temp_is_created_with_the_mode_not_chmodded_to_it() {
        let root = unique_tmp("hd-create");
        std::fs::create_dir_all(&root).unwrap();
        let f = create_temp(&root.join(".t"), &std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(f.metadata().unwrap().permissions().mode() & 0o777, 0o600, "straight from the open");
        let _ = std::fs::remove_dir_all(&root);
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
        let machine = crate::paths::rice_dir("barcarolle");
        shellbridge::copy_dir_recursive(&r.templates.join("barcarolle"), &machine).unwrap();
        std::fs::create_dir_all(machine.join("history")).unwrap();
        std::fs::write(machine.join("history").join("head.json"), "{}").unwrap();
        std::fs::write(machine.join("notes.md"), "machine-only prose").unwrap();
        let same = r.run("barcarolle");
        assert_eq!(same.data.as_ref().unwrap()["stagedDiffers"], false, "history/ and prose are not the dress");

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

    #[test]
    fn a_line_inside_a_block_comment_counts_as_none() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let text = "{\n  /* old:\n  habit.rice.declared = \"sonata\";\n  */\n}\n";
        let r = rig("hd-block", text, &["sonata", "barcarolle"]);
        let out = r.run("barcarolle");
        assert_eq!(Rig::reason(&out), "host-line-count");
        assert_eq!(out.data.as_ref().unwrap()["count"], 0);
        assert_eq!(r.host_text(), text);

        let live = "{\n  /* a note */\n  habit.rice.declared = \"sonata\";\n}\n";
        assert_eq!(declared_values(live).len(), 1, "a closed comment leaves the next line live");
    }

    #[test]
    fn a_line_inside_an_indented_string_counts_as_none() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let text = "{\n  environment.etc.x.text = ''\n  habit.rice.declared = \"sonata\";\n  it's ''' and ''${x}\n  '';\n}\n";
        let r = rig("hd-indented", text, &["sonata", "barcarolle"]);
        let out = r.run("barcarolle");
        assert_eq!(Rig::reason(&out), "host-line-count");
        assert_eq!(out.data.as_ref().unwrap()["count"], 0);
        assert_eq!(r.host_text(), text);

        let after = format!("{text}  habit.rice.declared = \"sonata\";\n");
        assert_eq!(declared_values(&after).len(), 1, "the string's close leaves the next line live");
        let quoted = "{\n  x = \"a\n  habit.rice.declared = \\\"sonata\\\";\n  b\";\n}\n";
        assert_eq!(declared_values(quoted).len(), 0, "a multi-line \"…\" string is not code either");
        assert_eq!(declared_values("# x /* not a comment opener\nhabit.rice.declared = \"s\";\n").len(), 1);
    }

    #[test]
    fn a_file_edited_while_declaring_is_refused_and_the_edit_kept() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-race", HOST, &["sonata", "barcarolle"]);
        let edited = format!("{HOST}# someone else's edit\n");
        let host = r.host.clone();
        let out = declare_racing("barcarolle", Some(r.host.clone()), Some(r.templates.clone()), &|| {
            std::fs::write(&host, &edited).unwrap()
        });
        assert_eq!(out.status, Status::Error, "{out:?}");
        assert_eq!(Rig::reason(&out), "host-config-changed");
        assert!(out.message.contains("edited while declaring"), "{}", out.message);
        assert_eq!(r.host_text(), edited, "the other edit survives");
        assert_eq!(std::fs::read_dir(r.host.parent().unwrap()).unwrap().count(), 1, "no temp left");
    }

    #[test]
    fn a_rice_without_its_own_rice_nix_is_refused() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-lender", HOST, &["sonata", "lender"]);
        std::fs::remove_file(r.templates.join("lender").join("rice.nix")).unwrap();
        let out = r.run("lender");
        assert_eq!(out.status, Status::Error);
        assert_eq!(Rig::reason(&out), "no-rice-nix");
        assert_eq!(r.host_text(), HOST);
    }

    #[test]
    fn no_templates_says_the_rice_list_is_unknown() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let r = rig("hd-notemplates", HOST, &["sonata"]);
        let out = declare("barcarolle", Some(r.host.clone()), None);
        assert_eq!(Rig::reason(&out), "not-built-in");
        let why = out.data.as_ref().unwrap().pointer("/refusal/why").and_then(Value::as_str).unwrap_or("").to_string();
        assert!(why.contains("the build's rice list is unknown"), "{why}");
        assert!(!why.contains("carries none"), "{why}");
    }
}
