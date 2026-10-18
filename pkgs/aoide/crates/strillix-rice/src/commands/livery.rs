//! `livery emit` / `livery resolve` / `livery lint` — the native livery-engine
//! command group (LIVERY-MERGE.md Phase 1): the former standalone token CLI's
//! surface, native, inside aoide's `Invocation`/`Outcome` shell.
//!
//! The handlers carry the engine's raw byte output in `data["stdout"]` — the
//! exact bytes the Node CLI printed (resolve JSON, shell-quoted hyprctl
//! lines, the OSC stream, the compact `{ok, …}` envelopes) — so the CLI door
//! can print them raw (see the `livery` special case in `cli/src/lib.rs`,
//! mirroring `schema`/`guide`) and the MCP/A2A doors still get the
//! structured envelope.

use crate::livery;
use crate::livery::emit::hyprctl::render_lines;
use crate::livery::emit::{EmitOpts, EmitOutput, emitter};
use crate::livery::json::{JVal, write_compact};
use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use aoide_storage::fs as shellbridge;
use serde_json::json;
use std::path::{Path, PathBuf};
use aoide_protocol::Door;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["livery", "emit"],
        summary: "Emit the fully-resolved livery set through a backend: stage JSON, hyprctl keyword lines, terminal OSC sequences, a file template ({{group.key}}), kitty colours, or the lock screen's hyprlock dress.",
        args: [
            arg!("target", "string", true, "Emitter backend: stage, hyprctl, osc, file, kitty, or hyprlock."),
            arg!("name", "string", false, "Song name or path to a livery file; defaults to the staged livery."),
        ],
        flags: [
            flag!("out", "string", "Write the emitted bytes to PATH atomically instead of stdout."),
            flag!("template", "string", "Template for the file backend ({{palette.bg}}-style placeholders)."),
            flag!("song-dir", "string", "hyprlock target: the absolute song folder whose lockscreen.<ext> the lock screen draws; without one the song dresses nothing."),
        ],
        gated: false,
        implemented: true,
        handler: handle_livery_emit,
    ));
    r.insert(cmd!(
        path: ["livery", "resolve"],
        summary: "Resolve a livery file to the flat, fully-resolved set (aliases deref'd, component fallbacks applied) and print it.",
        args: [arg!("name", "string", false, "Song name or path to a livery file; defaults to the staged livery.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_livery_resolve,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
    r.insert(cmd!(
        path: ["livery", "lint"],
        summary: "Validate a livery file against the closed v0 schema.",
        args: [arg!("name", "string", false, "Song name or path to a livery file; defaults to the staged livery.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_livery_lint,
        doors: &[Door::Cli, Door::Daemon, Door::Mcp],
    ));
}

/// Resolve the livery file a `livery` command should act on — the same seam as
/// `rice lint`, but RE-IMPLEMENTED here (not reusing
/// `resolve_rice_notes` in `commands/rice.rs`): `resolve_notes` is the
/// `livery`-local copy with a `skip` offset for the command's own leading
/// positionals — two implementations, one resolution rule:
///
/// * **no arg** — the staged notes (`<stage>/livery.json`) if present, else
///   a usage error (exit 2),
/// * **an arg that names an existing file** — taken as a literal path,
/// * **otherwise the arg is a committed-song NAME** →
///   `<song>/rices/<name>/livery.json`.
///
/// `skip` offsets past the command's own leading positionals (`livery emit`
/// takes `<target>` first, so its livery name lives at args[1]).
fn resolve_notes(inv: &Invocation, cmd: &str, skip: usize) -> Result<PathBuf, Outcome> {
    match inv.args.get(skip) {
        None => {
            let staged = shellbridge::stage_dir().join("livery.json");
            if staged.is_file() {
                Ok(staged)
            } else {
                Err(Outcome::usage(
                    cmd,
                    format!(
                        "no livery named and no staged livery at {}; \
                         usage: aoide {cmd} [<name>|<path>] [--json]",
                        staged.display()
                    ),
                )
                .with_data(json!({
                    "reason": "no-staged-livery",
                    "expected": staged.to_string_lossy(),
                })))
            }
        }
        Some(arg) => {
            // An existing path wins as a literal; otherwise treat it as a name.
            let literal = PathBuf::from(arg);
            if literal.is_file() {
                Ok(literal)
            } else {
                Ok(shellbridge::rice_notes(arg))
            }
        }
    }
}

/// Read + parse a livery file, with the Node engine's exact failure strings
/// (`cannot read {file}: {e}` / `invalid JSON in {file}: {e}`).
fn read_notes(target: &Path, cmd: &str) -> Result<serde_json::Value, Outcome> {
    let raw = match std::fs::read_to_string(target) {
        Ok(s) => s,
        Err(e) => {
            return Err(Outcome::error(
                cmd,
                format!("cannot read {}: {e}", target.display()),
            ));
        }
    };
    match serde_json::from_str(&raw) {
        Ok(v) => Ok(v),
        Err(e) => {
            Err(Outcome::error(
                cmd,
                format!("invalid JSON in {}: {e}", target.display()),
            ))
        }
    }
}

/// The Node lint envelope, byte-faithful (`{"ok":true,"schemaVersion":"0"}`
/// / `{"ok":false,"errors":[…]}` — compact, ok-first), as a string + "\n".
fn lint_envelope(ok: bool, errors: &[String]) -> String {
    let mut out = String::new();
    if ok {
        write_compact(
            &mut out,
            &JVal::obj(vec![
                ("ok".into(), JVal::boolean(true)),
                ("schemaVersion".into(), JVal::str(livery::SCHEMA_VERSION)),
            ]),
        );
    } else {
        write_compact(
            &mut out,
            &JVal::obj(vec![
                ("ok".into(), JVal::boolean(false)),
                (
                    "errors".into(),
                    JVal::arr(errors.iter().map(|e| JVal::str(e)).collect()),
                ),
            ]),
        );
    }
    out.push('\n');
    out
}

/// The shared validation-failure envelope (exit 1, never 0).
fn validation_failure(cmd: &str, target: &Path, errors: &[String]) -> Outcome {
    Outcome::error(cmd, "livery schema validation reported problems").with_data(json!({
        "ok": false,
        "errors": errors,
        "livery": target.to_string_lossy(),
        "stdout": lint_envelope(false, errors),
    }))
}

/// `livery lint [<name>|<path>]` — validate against the closed v0 schema.
fn handle_livery_lint(inv: &Invocation) -> Outcome {
    let target = match resolve_notes(inv, "livery.lint", 0) {
        Ok(p) => p,
        Err(o) => return o,
    };
    let notes = match read_notes(&target, "livery.lint") {
        Ok(v) => v,
        Err(o) => return o,
    };
    let v = livery::lint(&notes);
    if v.ok {
        Outcome::ok("livery.lint", "livery schema validation passed").with_data(json!({
            "ok": true,
            "schemaVersion": livery::SCHEMA_VERSION,
            "livery": target.to_string_lossy(),
            "stdout": lint_envelope(true, &[]),
        }))
    } else {
        validation_failure("livery.lint", &target, &v.errors)
    }
}

/// `livery resolve [<name>|<path>]` — the flat, fully-resolved livery set.
fn handle_livery_resolve(inv: &Invocation) -> Outcome {
    let target = match resolve_notes(inv, "livery.resolve", 0) {
        Ok(p) => p,
        Err(o) => return o,
    };
    let notes = match read_notes(&target, "livery.resolve") {
        Ok(v) => v,
        Err(o) => return o,
    };
    let v = livery::lint(&notes);
    if !v.ok {
        return validation_failure("livery.resolve", &target, &v.errors);
    }
    match livery::resolve(&notes) {
        Ok(r) => {
            let mut stdout = livery::resolve::to_json_string(&r);
            stdout.push('\n');
            Outcome::ok("livery.resolve", "resolved").with_data(json!({
                "livery": target.to_string_lossy(),
                "stdout": stdout,
            }))
        }
        Err(e) => Outcome::error("livery.resolve", e.to_string()).with_data(json!({
            "reason": "resolve-failed",
            "livery": target.to_string_lossy(),
        })),
    }
}

/// `livery emit <target> [<name>|<path>] [--out PATH] [--template STR]
/// [--song-dir DIR]` — run the resolved set through one backend and print (or
/// atomically write) its bytes. `--out` generalizes the Node `emit stage
/// --out` contract to every target; `--song-dir` is the `hyprlock` target's
/// song folder, searched for the lock picture the song ships.
fn handle_livery_emit(inv: &Invocation) -> Outcome {
    let target = match inv.args.first() {
        Some(t) => t.clone(),
        None => {
            return Outcome::usage(
                "livery.emit",
                "usage: aoide livery emit <target> [<name>|<path>] [--out PATH] [--template STR] [--song-dir DIR] [--json]",
            )
            .with_data(json!({ "reason": "missing-target" }));
        }
    };
    let notes_path = match resolve_notes(inv, "livery.emit", 1) {
        Ok(p) => p,
        Err(o) => return o,
    };
    let notes = match read_notes(&notes_path, "livery.emit") {
        Ok(v) => v,
        Err(o) => return o,
    };
    let v = livery::lint(&notes);
    if !v.ok {
        return validation_failure("livery.emit", &notes_path, &v.errors);
    }
    let resolved = match livery::resolve(&notes) {
        Ok(r) => r,
        Err(e) => {
            return Outcome::error("livery.emit", e.to_string()).with_data(json!({
                "reason": "resolve-failed",
                "livery": notes_path.to_string_lossy(),
            }));
        }
    };
    let backend = match emitter(&target) {
        Some(b) => b,
        None => {
            return Outcome::usage(
                "livery.emit",
                format!("emit: unknown target \"{target}\" (stage|hyprctl|osc|file|kitty|hyprlock)"),
            )
            .with_data(json!({ "reason": "unknown-target", "target": target }));
        }
    };
    let template = inv.flags.get("template").map(|s| s.as_str());
    let image = inv
        .flags
        .get("song-dir")
        .and_then(|dir| crate::cover::lockscreen_image(Path::new(dir)));
    let opts = EmitOpts { template, image: image.as_deref() };
    let output = match backend.emit(&resolved, &opts) {
        Ok(o) => o,
        Err(e) => {
            return Outcome::error("livery.emit", e.to_string()).with_data(json!({
                "reason": "emit-failed",
                "target": target,
            }));
        }
    };
    let stdout = match &output {
        EmitOutput::Json(v) => serde_json::to_string_pretty(v).unwrap_or_default() + "\n",
        EmitOutput::Lines(cmds) => render_lines(cmds),
        EmitOutput::Text(s) => s.clone(),
    };

    if let Some(out_path) = inv.flags.get("out") {
        if let Err(e) = shellbridge::atomic_write(Path::new(out_path), &stdout) {
            return Outcome::error("livery.emit", format!("failed to write {out_path}: {e}"))
                .with_data(json!({ "reason": "write-failed", "target": out_path }));
        }
        // The Node `--out` envelope: `{"ok":true,"wrote":<path>}`.
        let mut envelope = String::new();
        write_compact(
            &mut envelope,
            &JVal::obj(vec![
                ("ok".into(), JVal::boolean(true)),
                ("wrote".into(), JVal::str(out_path)),
            ]),
        );
        envelope.push('\n');
        return Outcome::ok("livery.emit", format!("wrote {out_path}"))
            .changed(vec![out_path.clone()])
            .with_data(json!({
                "ok": true,
                "wrote": out_path,
                "livery": notes_path.to_string_lossy(),
                "stdout": envelope,
            }));
    }

    Outcome::ok("livery.emit", format!("emitted {target}")).with_data(json!({
        "target": target,
        "livery": notes_path.to_string_lossy(),
        "stdout": stdout,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::output::Status;
    use aoide_test_support::{inv, unique_tmp};

    const FIXTURE: &str = "tests/fixtures/valid-base16.json";

    fn emit_hyprlock(strillix_dir: Option<&Path>) -> Outcome {
        let mut i = inv(&["livery", "emit"], &["hyprlock", FIXTURE]);
        if let Some(dir) = strillix_dir {
            i.flags.insert("song-dir".into(), dir.to_string_lossy().into_owned());
        }
        handle_livery_emit(&i)
    }

    fn stdout(out: &Outcome) -> String {
        out.data.as_ref().unwrap()["stdout"].as_str().unwrap().to_string()
    }

    #[test]
    fn hyprlock_draws_the_song_folders_lock_picture_and_nothing_without_one() {
        let song = unique_tmp("livery-emit-hyprlock");
        let out = emit_hyprlock(Some(&song));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(stdout(&out), crate::livery::emit::hyprlock::NO_DRESS, "no picture, no dress");
        assert_eq!(stdout(&emit_hyprlock(None)), crate::livery::emit::hyprlock::NO_DRESS);

        std::fs::write(song.join("lockscreen.png"), b"").unwrap();
        let out = emit_hyprlock(Some(&song));
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        let want = format!("$aoide_lock_image = {}", song.join("lockscreen.png").display());
        let text = stdout(&out);
        assert!(text.lines().any(|l| l == want), "{text}");
        assert_eq!(text.lines().filter(|l| l.starts_with("$aoide_lock_")).count(), 10, "{text}");
        let _ = std::fs::remove_dir_all(&song);
    }

    #[test]
    fn hyprlock_writes_the_same_bytes_through_out() {
        let song = unique_tmp("livery-emit-hyprlock-out");
        std::fs::write(song.join("lockscreen.webp"), b"").unwrap();
        let dst = song.join("out").join("hyprlock.conf");
        let mut i = inv(&["livery", "emit"], &["hyprlock", FIXTURE]);
        i.flags.insert("song-dir".into(), song.to_string_lossy().into_owned());
        i.flags.insert("out".into(), dst.to_string_lossy().into_owned());
        let out = handle_livery_emit(&i);
        assert_eq!(out.status, Status::Ok, "{:?}", out.data);
        assert_eq!(
            std::fs::read_to_string(&dst).unwrap(),
            stdout(&emit_hyprlock(Some(&song)))
        );
        let _ = std::fs::remove_dir_all(&song);
    }
}
