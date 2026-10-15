//! `lyra clipboard` — remove entries from the cliphist clipboard history.
//!
//! - `clipboard delete <id>` removes one entry by its numeric cliphist id and
//!   its cached thumbnail.
//! - `clipboard clear` wipes the whole history and every cached thumbnail.
//!
//! Both drive the `cliphist` on `PATH` (the `clipboard` dendrite installs it;
//! a host running its own watcher brings its own). The launcher's clipboard
//! backend (`pkgs/lyra-shell/qml/AoideClipboard.qml`) runs these commands
//! rather than `cliphist delete`/`wipe` itself, so removing history stays
//! reachable from a terminal with every `.qml` deleted.
//!
//! Only an id travels to cliphist: `delete` writes `<id>\t` to `cliphist
//! delete`'s stdin, the shape it parses from a `cliphist list` line. The
//! history's text is read only to learn which ids exist and never reaches
//! output. Thumbnails live in `$XDG_CACHE_HOME/aoide-clipboard/previews/<id>`
//! (`~/.cache` when unset), the directory `AoideClipboard.qml` decodes into.

use crate::dispatch::Invocation;
use crate::output::{io_cause, Fix, Kind, Outcome};
use crate::registry::{arg, cmd, Registry};
use aoide_conduct::graph::clean_line;
use serde_json::json;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CLIPHIST: &str = "cliphist";

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["clipboard", "delete"],
        summary: "Delete one clipboard-history entry by its numeric cliphist id, and its cached thumbnail.",
        args: [arg!("id", "string", true, "A numeric cliphist id, as the first column of `cliphist list`.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_delete,
        examples: ["clipboard delete 42", "clipboard delete 42 --json"],
        brief: "Delete one clipboard-history entry by id.",
    ));
    r.insert(cmd!(
        path: ["clipboard", "clear"],
        summary: "Wipe the whole clipboard history (cliphist wipe) and every cached thumbnail.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_clear,
        examples: ["clipboard clear", "clipboard clear --json"],
        brief: "Wipe the whole clipboard history.",
    ));
}

fn handle_delete(inv: &Invocation) -> Outcome {
    let cmd = "clipboard.delete";
    let typed = inv.args.first().map(String::as_str).unwrap_or_default();
    let Some(id) = parse_id(typed) else {
        return refuse(
            cmd,
            Kind::Usage,
            "bad-id",
            format!("`{}` is not a clipboard id", clean_line(typed)),
            "a cliphist id is a whole number, the first column of `cliphist list`",
            Fix::Run("cliphist list".into()),
        );
    };
    let listed = match cliphist(cmd, &["list"], None) {
        Ok(out) => out,
        Err(o) => return o,
    };
    if !held_ids(&listed).contains(&id) {
        return refuse(
            cmd,
            Kind::Refused,
            "no-such-entry",
            format!("no clipboard entry {id}"),
            "cliphist holds no entry with that id; it may already be deleted",
            Fix::None("the entry is already gone"),
        );
    }
    if let Err(o) = cliphist(cmd, &["delete"], Some(format!("{id}\t\n"))) {
        return o;
    }
    let preview = preview_dir().join(id.to_string());
    let removed = match std::fs::remove_file(&preview) {
        Ok(()) => true,
        Err(e) if e.kind() == ErrorKind::NotFound => false,
        Err(e) => return thumbnail_refusal(cmd, &preview, &e),
    };
    Outcome::ok(cmd, format!("deleted clipboard entry {id}")).with_data(json!({
        "id": id.to_string(),
        "previewRemoved": removed,
    }))
}

fn handle_clear(_inv: &Invocation) -> Outcome {
    let cmd = "clipboard.clear";
    if let Err(o) = cliphist(cmd, &["wipe"], None) {
        return o;
    }
    let dir = preview_dir();
    let count = std::fs::read_dir(&dir).map_or(0, |d| d.count());
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return thumbnail_refusal(cmd, &dir, &e),
    }
    Outcome::ok(cmd, "cleared the clipboard history").with_data(json!({ "previewsRemoved": count }))
}

/// A whole number that fits cliphist's u64 key; signs, spaces and leading `+` are refused.
fn parse_id(typed: &str) -> Option<u64> {
    if typed.is_empty() || !typed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    typed.parse().ok()
}

fn held_ids(listed: &str) -> Vec<u64> {
    listed.lines().filter_map(|l| l.split_once('\t')).filter_map(|(id, _)| parse_id(id)).collect()
}

/// `$XDG_CACHE_HOME/aoide-clipboard/previews`, `~/.cache` when the variable is empty or relative.
fn preview_dir() -> PathBuf {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty()).map(PathBuf::from).filter(|p| p.is_absolute());
    let cache = var("XDG_CACHE_HOME").unwrap_or_else(|| var("HOME").unwrap_or_else(aoide_protocol::aoide_home).join(".cache"));
    cache.join("aoide-clipboard/previews")
}

/// Runs `cliphist <args>`, feeding `stdin` when given; its stdout on success.
fn cliphist(cmd: &str, args: &[&str], stdin: Option<String>) -> Result<String, Outcome> {
    let verb = args.join(" ");
    let mut child = Command::new(CLIPHIST)
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                refuse(
                    cmd,
                    Kind::Refused,
                    "cliphist-missing",
                    "cliphist is not installed",
                    "no `cliphist` on PATH holds the clipboard history",
                    Fix::Set("habit.dendrites.clipboard.enable = true".into()),
                )
            } else {
                failed(cmd, &verb, format!("spawn cliphist: {e}"))
            }
        })?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(text.as_bytes()).map_err(|e| failed(cmd, &verb, format!("write cliphist stdin: {e}")))?;
    }
    let out = child.wait_with_output().map_err(|e| failed(cmd, &verb, format!("wait cliphist: {e}")))?;
    if !out.status.success() {
        return Err(failed(cmd, &verb, String::from_utf8_lossy(&out.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn failed(cmd: &str, verb: &str, detail: String) -> Outcome {
    refuse(
        cmd,
        Kind::Failed,
        "cliphist-failed",
        format!("`cliphist {verb}` failed"),
        "cliphist exited with an error; its own words are in `detail`",
        Fix::Run(format!("cliphist {verb}")),
    )
    .with_detail(detail)
}

fn thumbnail_refusal(cmd: &str, path: &std::path::Path, e: &std::io::Error) -> Outcome {
    let cause = io_cause("remove", path, e);
    refuse(cmd, Kind::Failed, "preview-remove-failed", "the history changed but a thumbnail stayed", cause.why, Fix::None("the cache directory must be writable"))
        .with_detail(cause.detail)
}

fn refuse(cmd: &str, kind: Kind, reason: &str, what: impl Into<String>, why: impl Into<String>, fix: Fix) -> Outcome {
    Outcome::refuse(cmd, kind, what, why, fix).with_fields(json!({ "reason": reason }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::output::Status;
    use aoide_test_support::{env_lock, inv, unique_tmp, EnvSaver};
    use serde_json::Value;
    use std::path::Path;

    /// A scratch HOME/XDG_CACHE_HOME and a PATH holding only a `cliphist`
    /// stand-in, so the real cliphist and the real history are unreachable.
    /// The stand-in keeps its history in `db` beside itself, logs each argv
    /// to `argv.log` and each stdin to `stdin.log`, and fails any verb named
    /// by a `fail-<verb>` file. Builtins only: `PATH` holds nothing else.
    struct Rig {
        root: PathBuf,
        _env: EnvSaver,
    }

    impl Rig {
        fn new(tag: &str, history: &str) -> Rig {
            let env = EnvSaver::capture(&["PATH", "HOME", "XDG_CACHE_HOME", "CLIPHIST_DB_PATH"]);
            let root = unique_tmp(tag);
            let bin = root.join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::create_dir_all(root.join("cache/aoide-clipboard/previews")).unwrap();
            std::fs::write(root.join("db"), history).unwrap();
            let stub = bin.join("cliphist");
            std::fs::write(
                &stub,
                r#"#!/bin/sh
here="${0%/bin/cliphist}"
echo "$*" >> "$here/argv.log"
if [ -e "$here/fail-$1" ]; then echo "cliphist: $1 broke" >&2; exit 1; fi
case "$1" in
list) while IFS= read -r line; do printf '%s\n' "$line"; done < "$here/db" ;;
delete)
  IFS= read -r input
  printf '%s\n' "$input" >> "$here/stdin.log"
  id="${input%%	*}"
  kept=""
  while IFS= read -r line; do
    [ "${line%%	*}" = "$id" ] || kept="$kept$line
"
  done < "$here/db"
  printf '%s' "$kept" > "$here/db" ;;
wipe) : > "$here/db" ;;
*) exit 2 ;;
esac
"#,
            )
            .unwrap();
            std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
            std::env::set_var("PATH", &bin);
            std::env::set_var("HOME", root.join("home"));
            std::env::set_var("XDG_CACHE_HOME", root.join("cache"));
            std::env::set_var("CLIPHIST_DB_PATH", root.join("db"));
            Rig { root, _env: env }
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.root.join(rel)
        }

        fn read(&self, rel: &str) -> String {
            std::fs::read_to_string(self.path(rel)).unwrap_or_default()
        }

        fn preview(&self, id: &str) -> PathBuf {
            let p = self.path("cache/aoide-clipboard/previews").join(id);
            std::fs::write(&p, b"png").unwrap();
            p
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn reason(o: &Outcome) -> &str {
        o.data.as_ref().and_then(|d| d["reason"].as_str()).unwrap_or_default()
    }

    const HISTORY: &str = "42\tsecret text\n7\t[[ binary data 1 KiB png 2x2 ]]\n";

    #[test]
    fn delete_sends_only_the_id_and_removes_that_thumbnail() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("clip-delete", HISTORY);
        let mine = rig.preview("42");
        let other = rig.preview("7");
        let o = handle_delete(&inv(&["clipboard", "delete"], &["42"]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(o.message, "deleted clipboard entry 42");
        assert_eq!(o.data.unwrap(), json!({ "id": "42", "previewRemoved": true }));
        assert_eq!(rig.read("argv.log"), "list\ndelete\n");
        assert_eq!(rig.read("stdin.log"), "42\t\n", "only the id reaches cliphist");
        assert_eq!(rig.read("db"), "7\t[[ binary data 1 KiB png 2x2 ]]\n");
        assert!(!mine.exists() && other.exists());
    }

    #[test]
    fn delete_without_a_thumbnail_says_so() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _rig = Rig::new("clip-delete-nothumb", HISTORY);
        let o = handle_delete(&inv(&["clipboard", "delete"], &["7"]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(o.data.unwrap()["previewRemoved"], false);
    }

    #[test]
    fn the_history_text_never_reaches_the_output() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _rig = Rig::new("clip-delete-quiet", HISTORY);
        for o in [handle_delete(&inv(&["clipboard", "delete"], &["42"])), handle_delete(&inv(&["clipboard", "delete"], &["9"]))] {
            let text = format!("{} {}", o.message, serde_json::to_string(&o.data).unwrap());
            assert!(!text.contains("secret"), "{text}");
        }
    }

    #[test]
    fn a_non_numeric_id_is_a_usage_refusal_before_cliphist_runs() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("clip-bad-id", HISTORY);
        for typed in ["", "abc", "-1", "+4", " 4", "4\t", "1.5", "99999999999999999999", "4\n\u{1b}]0;x"] {
            let o = handle_delete(&inv(&["clipboard", "delete"], &[typed]));
            assert_eq!(o.status, Status::Usage, "{typed:?}");
            assert_eq!(reason(&o), "bad-id");
            assert!(!o.message.chars().any(char::is_control), "{:?}", o.message);
        }
        assert_eq!(rig.read("argv.log"), "", "cliphist never ran");
    }

    #[test]
    fn an_id_cliphist_does_not_hold_is_refused_and_nothing_is_deleted() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("clip-unknown", HISTORY);
        let thumb = rig.preview("9");
        let o = handle_delete(&inv(&["clipboard", "delete"], &["9"]));
        assert_eq!(o.status, Status::Error);
        assert_eq!(reason(&o), "no-such-entry");
        assert_eq!(o.data.unwrap()["refusal"]["kind"], "refused");
        assert_eq!(rig.read("argv.log"), "list\n");
        assert_eq!(rig.read("db"), HISTORY);
        assert!(thumb.exists());
    }

    #[test]
    fn a_leading_zero_names_the_same_entry() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("clip-zero", HISTORY);
        let o = handle_delete(&inv(&["clipboard", "delete"], &["007"]));
        assert_eq!(o.data.unwrap()["id"], "7");
        assert_eq!(rig.read("stdin.log"), "7\t\n");
    }

    #[test]
    fn clear_wipes_the_history_and_every_thumbnail() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("clip-clear", HISTORY);
        rig.preview("42");
        rig.preview(".tmp.7.123");
        let o = handle_clear(&inv(&["clipboard", "clear"], &[]));
        assert_eq!(o.status, Status::Ok, "{o:?}");
        assert_eq!(o.data.unwrap(), json!({ "previewsRemoved": 2 }));
        assert_eq!(rig.read("argv.log"), "wipe\n");
        assert_eq!(rig.read("db"), "");
        assert!(!rig.path("cache/aoide-clipboard/previews").exists());
        assert!(rig.path("cache/aoide-clipboard").exists(), "only the previews dir goes");

        let again = handle_clear(&inv(&["clipboard", "clear"], &[]));
        assert_eq!(again.status, Status::Ok, "an empty history clears again: {again:?}");
        assert_eq!(again.data.unwrap()["previewsRemoved"], 0);
    }

    #[test]
    fn a_missing_cliphist_is_refused_with_the_setting_that_installs_it() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("clip-missing", HISTORY);
        std::fs::remove_file(rig.path("bin/cliphist")).unwrap();
        for o in [handle_delete(&inv(&["clipboard", "delete"], &["42"])), handle_clear(&inv(&["clipboard", "clear"], &[]))] {
            assert_eq!(o.status, Status::Error, "{o:?}");
            assert_eq!(reason(&o), "cliphist-missing");
            assert_eq!(o.data.unwrap()["refusal"]["fix"]["set"], "habit.dendrites.clipboard.enable = true");
        }
        assert!(rig.path("cache/aoide-clipboard/previews").exists(), "no thumbnail touched");
    }

    #[test]
    fn a_failing_cliphist_is_a_failure_with_its_stderr_in_detail_and_keeps_thumbnails() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let rig = Rig::new("clip-fail", HISTORY);
        let thumb = rig.preview("42");
        for verb in ["delete", "wipe"] {
            std::fs::write(rig.path(&format!("fail-{verb}")), "").unwrap();
        }
        let checks: [(Outcome, &str); 2] = [
            (handle_delete(&inv(&["clipboard", "delete"], &["42"])), "delete"),
            (handle_clear(&inv(&["clipboard", "clear"], &[])), "wipe"),
        ];
        for (o, verb) in checks {
            assert_eq!(o.status, Status::Error, "{o:?}");
            assert_eq!(reason(&o), "cliphist-failed");
            let data: Value = o.data.unwrap();
            assert_eq!(data["refusal"]["kind"], "failed");
            assert_eq!(data["detail"], format!("cliphist: {verb} broke"));
        }
        assert!(thumb.exists());
        assert_eq!(rig.read("db"), HISTORY);
    }

    #[test]
    fn the_preview_dir_follows_xdg_cache_home_then_home() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["HOME", "XDG_CACHE_HOME"]);
        std::env::set_var("HOME", "/h");
        std::env::set_var("XDG_CACHE_HOME", "/c");
        assert_eq!(preview_dir(), Path::new("/c/aoide-clipboard/previews"));
        std::env::set_var("XDG_CACHE_HOME", "relative");
        assert_eq!(preview_dir(), Path::new("/h/.cache/aoide-clipboard/previews"));
        std::env::remove_var("XDG_CACHE_HOME");
        assert_eq!(preview_dir(), Path::new("/h/.cache/aoide-clipboard/previews"));
    }
}
