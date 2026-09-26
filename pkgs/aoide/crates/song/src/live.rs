//! Live-apply helpers — the compositor and terminal halves of `rice stage`.
//!
//! `handle_rice_stage` (dispatch.rs) already hot-reloads the palette by
//! staging `stage/livery.json`; Quickshell's own FileView watches that file
//! and needs no compositor call. Geometry (gaps/border-size/rounding/blur)
//! and the border *colours* have no such watcher on the Hyprland side, so
//! this module turns the staged notes into a single best-effort
//! `hyprctl --batch` keyword list and (when on Hyprland) runs it.
//!
//! Every field here is live-settable via `hyprctl keyword` — there is
//! deliberately no `hyprctl reload` anywhere in this seam. `reload` re-reads
//! `hyprland.conf` from disk; nothing here rewrites that file (the baked
//! config is still the build-time source for the NEXT compositor start), so
//! a reload would find nothing new to pick up and would needlessly reset
//! every OTHER live-tweaked keyword a user has set out-of-band.
//!
//! The terminal half (below the compositor functions) renders the staged
//! terminal colour file through the livery engine's `kitty` emitter and
//! pushes a written one to every open kitty over its control socket. The
//! write itself stays with the caller (`commands::rice::stage_terminal_colors`),
//! like the stage file's: this module holds no storage dependency.

use crate::livery::schema;
use serde_json::Value;

/// Build the `hyprctl keyword …` list for one staged notes document, in the
/// fixed order CONTRACTS.md §1's geometry table lists them (gaps, border
/// size, border colours, rounding, blur), then the two hyprglass switches.
///
/// Only emits a keyword for a field that actually resolves to a concrete
/// value:
///
/// * **Border colours** (`window.border` / `window.borderInactive`) are the
///   component tier, which the stage file always carries fully resolved
///   (CONTRACTS.md §4 — "Quickshell reads concrete colours, never `null`"),
///   so these two keywords are effectively unconditional for any valid
///   staged notes document.
/// * **Geometry** (`geometry.*`) is additive-optional (§1): a notes file with
///   no `geometry` block, or a block with a `null` field, means "this song
///   never opted in" — we skip that keyword rather than asserting the
///   compositor's own fallback constant (8/6/2/0/true/8/3). Asserting the
///   fallback here would fight a host's baked `hyprland.conf` (or a user's
///   own live tweak) on every preview of a song that carries no geometry
///   opinion; skipping lets the existing value stand.
pub fn geometry_keywords(notes: &Value) -> Vec<String> {
    let geo = notes.get("geometry");
    let mut out = Vec::new();

    push_int(&mut out, geo, "gapsOut", "general:gaps_out");
    push_int(&mut out, geo, "gapsIn", "general:gaps_in");
    push_int(&mut out, geo, "borderSize", "general:border_size");

    // `schema::is_hex` lints BEFORE the value ever reaches `to_hyprland_rgb`
    // — this string is interpolated straight into a `;`-joined `hyprctl
    // --batch` command, so anything that isn't a clean `#rrggbb`/`rrggbb` hex
    // (e.g. `0; dispatch exec <cmd>`) must never reach it. A staged notes
    // document is normally already `rice lint`-clean by the time it lands
    // here, but this is the last line of defense in the actual hot path
    // (`handle_rice_stage` stages+applies without re-linting), so an invalid
    // value is silently skipped — same "no opinion" treatment as an absent
    // geometry field — rather than passed through or defaulted.
    if let Some(hex) = notes.pointer("/window/border").and_then(Value::as_str) {
        if schema::is_hex(hex) {
            out.push(format!(
                "keyword general:col.active_border {}",
                to_hyprland_rgb(hex)
            ));
        }
    }
    if let Some(hex) = notes.pointer("/window/borderInactive").and_then(Value::as_str) {
        if schema::is_hex(hex) {
            out.push(format!(
                "keyword general:col.inactive_border {}",
                to_hyprland_rgb(hex)
            ));
        }
    }

    push_int(&mut out, geo, "rounding", "decoration:rounding");
    push_bool01(&mut out, geo, "blurEnabled", "decoration:blur:enabled");
    push_int(&mut out, geo, "blurSize", "decoration:blur:size");
    push_int(&mut out, geo, "blurPasses", "decoration:blur:passes");

    // hyprglass follows the same switch: a song with blur off wants no glass
    // either, and one with blur on gets it back. Both of the plugin's own
    // enable keys are read per frame (static config pointers), so a keyword
    // turns the glass off live without unloading the plugin: `enabled` is the
    // global window-glass switch, `layers:enabled` the layer-surface one the
    // compositor facet turns on for the aoide-* namespaces. Last in the batch,
    // so on a host without the plugin loaded their refusal comes after every
    // core keyword has already applied.
    push_bool01(&mut out, geo, "blurEnabled", "plugin:hyprglass:enabled");
    push_bool01(&mut out, geo, "blurEnabled", "plugin:hyprglass:layers:enabled");

    out
}

fn push_int(out: &mut Vec<String>, geo: Option<&Value>, field: &str, keyword: &str) {
    if let Some(n) = geo.and_then(|g| g.get(field)).and_then(Value::as_i64) {
        out.push(format!("keyword {keyword} {n}"));
    }
}

fn push_bool01(out: &mut Vec<String>, geo: Option<&Value>, field: &str, keyword: &str) {
    if let Some(b) = geo.and_then(|g| g.get(field)).and_then(Value::as_bool) {
        out.push(format!("keyword {keyword} {}", if b { 1 } else { 0 }));
    }
}

/// `#RRGGBB` (or bare `RRGGBB`) → Hyprland's `rgb(RRGGBB)` colour syntax.
fn to_hyprland_rgb(hex: &str) -> String {
    format!("rgb({})", hex.trim_start_matches('#'))
}

/// Join a keyword list into the single `hyprctl --batch` payload string
/// (`"keyword a b; keyword c d; …"`).
pub fn batch_command(keywords: &[String]) -> String {
    keywords.join("; ")
}

/// Guarded, best-effort live-apply. Returns a short status string for the
/// caller's outcome envelope; NEVER a `Result` — a failed or absent `hyprctl`
/// must not fail `rice stage` (the stage file is already the source of
/// truth for the hot-reload half; this is best-effort on top of it).
///
/// Guard: only runs `hyprctl` when `$HYPRLAND_INSTANCE_SIGNATURE` is set
/// (off-Hyprland — headless, VM, or the common test path — is a silent
/// no-op) and there is at least one keyword to apply.
pub fn apply_live(keywords: &[String]) -> &'static str {
    if keywords.is_empty() {
        return "skipped (no geometry/border keywords resolved)";
    }
    let on_hyprland = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    if !on_hyprland {
        return "skipped (HYPRLAND_INSTANCE_SIGNATURE unset)";
    }
    match std::process::Command::new("hyprctl")
        .arg("--batch")
        .arg(batch_command(keywords))
        .output()
    {
        Ok(out) if out.status.success() => "applied",
        Ok(_) => "best-effort: hyprctl reported an error (stage file already updated)",
        Err(_) => "best-effort: hyprctl unavailable (stage file already updated)",
    }
}


// ── The terminal half ────────────────────────────────────────────────────
//
// kitty has no FileView: a colour file on disk reaches a NEW window through
// the kitty dendrite's `include` of it, and an OPEN window only when kitty is
// told. The telling goes through kitty's own control socket
// (`kitty @ set-colors`), never a raw OSC write to a pty — an OSC write
// interleaves with whatever the program in that pty is printing and gets
// eaten. The sockets are found by the path pattern the kitty dendrite
// configures (`listen_on unix:${XDG_RUNTIME_DIR}/kitty-{kitty_pid}`), a
// plain directory listing — no process-table or /proc walk.

/// The staged terminal colour file's name inside the stage dir — the runtime
/// root's contract path `$AOIDE_ROOT/song/stage/terminal-colors.conf`, the
/// file the kitty dendrite includes.
pub const TERMINAL_COLORS_FILE: &str = "terminal-colors.conf";

/// How long one `kitty @` call may take before it is abandoned. A socket
/// whose kitty is wedged must not stall `rice stage`.
const KITTY_CALL_LIMIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Render the kitty colour file one staged notes document implies, through
/// the livery engine's `kitty` emitter. `None` when the notes don't resolve
/// (a torn or reference-cyclic notes file) — the caller leaves the previous
/// file in place rather than writing a guess.
pub fn terminal_colors(notes: &Value) -> Option<String> {
    let r = crate::livery::resolve(notes).ok()?;
    Some(crate::livery::emit::kitty::emit_kitty(&r))
}

/// Every kitty control socket under `runtime_dir`: entries named
/// `kitty-<pid>` (digits only) that are unix sockets. Sorted, so the push
/// order is stable. An unreadable directory is simply no sockets.
pub fn kitty_sockets(runtime_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    use std::os::unix::fs::FileTypeExt;
    let Ok(entries) = std::fs::read_dir(runtime_dir) else {
        return Vec::new();
    };
    let mut out: Vec<std::path::PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("kitty-")
                .is_some_and(|pid| !pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit()))
                && e.file_type().is_ok_and(|t| t.is_socket())
        })
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// Run one `kitty @` call with a bounded wait, stdout captured on a reader
/// thread (an `ls` of a busy instance can outgrow the pipe buffer, which
/// would otherwise wedge the child until the deadline). `Err(NotFound)` when
/// `kitty` itself is not on `PATH`; `Ok(None)` when it ran but failed or
/// timed out; `Ok(Some(stdout))` on success.
fn kitty_call(args: &[&std::ffi::OsStr]) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("kitty")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = std::time::Instant::now() + KITTY_CALL_LIMIT;
    let ok = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let out = reader.join().unwrap_or_default();
    Ok(ok.then_some(out))
}

/// Count the windows in a `kitty @ ls` document (OS windows → tabs →
/// windows). `None` when it isn't that shape.
fn count_windows(ls: &[u8]) -> Option<usize> {
    let v: Value = serde_json::from_slice(ls).ok()?;
    Some(
        v.as_array()?
            .iter()
            .flat_map(|os| os.get("tabs").and_then(Value::as_array).into_iter().flatten())
            .map(|tab| tab.get("windows").and_then(Value::as_array).map_or(0, Vec::len))
            .sum(),
    )
}

/// Guarded, best-effort push of a colour file to every open kitty:
/// `kitty @ --to unix:<sock> set-colors --all --configured <file>` per
/// socket (`--configured` so a new window of that same instance opens in
/// the staged colours too), then an `ls` of the recoloured instance to
/// count its windows. Never a `Result` — the same tier as [`apply_live`]:
/// the colour file on disk is already the truth for every new window, and
/// this only brings the open ones along.
///
/// A quiet skip, not a failure, when there is nothing to push to: no
/// `$XDG_RUNTIME_DIR`, no `kitty-<pid>` socket under it, or no `kitty` on
/// `PATH`. The returned object is the outcome envelope's `terminal` data:
/// `{status, message, instances, windows, failed}`.
pub fn push_kitty_colors(conf: &std::path::Path) -> Value {
    let skip = |why: &str| {
        serde_json::json!({
            "status": "skipped", "message": why, "instances": 0, "windows": 0, "failed": 0,
        })
    };
    let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) else {
        return skip("XDG_RUNTIME_DIR unset");
    };
    let sockets = kitty_sockets(std::path::Path::new(&runtime_dir));
    if sockets.is_empty() {
        return skip("no kitty control socket open");
    }

    let (mut instances, mut windows, mut failed) = (0usize, 0usize, 0usize);
    for sock in &sockets {
        let mut to = std::ffi::OsString::from("unix:");
        to.push(sock.as_os_str());
        let set = kitty_call(&[
            "@".as_ref(),
            "--to".as_ref(),
            to.as_os_str(),
            "set-colors".as_ref(),
            "--all".as_ref(),
            "--configured".as_ref(),
            conf.as_os_str(),
        ]);
        match set {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return skip("kitty not on PATH");
            }
            Ok(Some(_)) => {
                instances += 1;
                let ls = kitty_call(&["@".as_ref(), "--to".as_ref(), to.as_os_str(), "ls".as_ref()]);
                windows += ls.ok().flatten().and_then(|b| count_windows(&b)).unwrap_or(0);
            }
            // A stale socket (its kitty gone), a refusal, or a timeout.
            _ => failed += 1,
        }
    }
    let status = match (instances, failed) {
        (0, _) => "failed",
        (_, 0) => "applied",
        _ => "partial",
    };
    serde_json::json!({
        "status": status,
        "message": format!(
            "recoloured {windows} kitty window(s) across {instances} instance(s){}",
            if failed > 0 { format!("; {failed} socket(s) did not answer") } else { String::new() }
        ),
        "instances": instances,
        "windows": windows,
        "failed": failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn full_geometry_and_window_produce_every_keyword_in_order() {
        let notes = json!({
            "window": { "border": "#89b4fa", "borderInactive": "#1e1e2e" },
            "geometry": {
                "gapsOut": 8, "gapsIn": 6, "borderSize": 2, "rounding": 0,
                "blurEnabled": true, "blurSize": 8, "blurPasses": 3
            }
        });
        assert_eq!(
            geometry_keywords(&notes),
            vec![
                "keyword general:gaps_out 8".to_string(),
                "keyword general:gaps_in 6".to_string(),
                "keyword general:border_size 2".to_string(),
                "keyword general:col.active_border rgb(89b4fa)".to_string(),
                "keyword general:col.inactive_border rgb(1e1e2e)".to_string(),
                "keyword decoration:rounding 0".to_string(),
                "keyword decoration:blur:enabled 1".to_string(),
                "keyword decoration:blur:size 8".to_string(),
                "keyword decoration:blur:passes 3".to_string(),
                "keyword plugin:hyprglass:enabled 1".to_string(),
                "keyword plugin:hyprglass:layers:enabled 1".to_string(),
            ]
        );
    }

    #[test]
    fn blur_disabled_emits_zero_not_the_word_false() {
        let notes = json!({ "geometry": { "blurEnabled": false } });
        assert_eq!(
            geometry_keywords(&notes),
            vec![
                "keyword decoration:blur:enabled 0".to_string(),
                "keyword plugin:hyprglass:enabled 0".to_string(),
                "keyword plugin:hyprglass:layers:enabled 0".to_string(),
            ]
        );
    }

    #[test]
    fn hyprglass_follows_blur_and_stays_silent_without_an_opinion() {
        // cadenza's shape: blur off, glass off, both live keywords.
        let off = geometry_keywords(&json!({ "geometry": { "blurEnabled": false, "rounding": 0 } }));
        assert!(off.contains(&"keyword plugin:hyprglass:enabled 0".to_string()));
        assert!(off.contains(&"keyword plugin:hyprglass:layers:enabled 0".to_string()));
        // A song that wants blur turns the glass back on.
        let on = geometry_keywords(&json!({ "geometry": { "blurEnabled": true } }));
        assert!(on.contains(&"keyword plugin:hyprglass:enabled 1".to_string()));
        assert!(on.contains(&"keyword plugin:hyprglass:layers:enabled 1".to_string()));
        // No geometry opinion, no glass keyword: the running value stands.
        let none = geometry_keywords(&json!({ "geometry": { "blurEnabled": null } }));
        assert!(none.iter().all(|k| !k.contains("hyprglass")), "{none:?}");
    }

    #[test]
    fn no_geometry_block_still_emits_the_always_resolved_border_colours() {
        let notes = json!({ "window": { "border": "#a07414", "borderInactive": "#3f867e" } });
        assert_eq!(
            geometry_keywords(&notes),
            vec![
                "keyword general:col.active_border rgb(a07414)".to_string(),
                "keyword general:col.inactive_border rgb(3f867e)".to_string(),
            ]
        );
    }

    #[test]
    fn null_geometry_fields_are_skipped_individually() {
        // Additive-optional (§1): a present-but-null field means "no opinion",
        // never the compositor's fallback constant.
        let notes = json!({
            "geometry": { "gapsOut": null, "gapsIn": 6, "borderSize": null }
        });
        assert_eq!(
            geometry_keywords(&notes),
            vec!["keyword general:gaps_in 6".to_string()]
        );
    }

    #[test]
    fn a_non_hex_border_is_never_interpolated_into_the_batch_command() {
        // `to_hyprland_rgb` interpolates this string directly into a
        // `;`-joined `hyprctl --batch` payload (see `batch_command`) — a
        // value shaped like a second command must be dropped, not passed
        // through, since a staged notes file may not have been re-linted by
        // the time it reaches this hot path (`handle_rice_stage`).
        let notes = json!({
            "window": { "border": "0; dispatch exec touch /tmp/pwned" }
        });
        assert!(
            geometry_keywords(&notes).is_empty(),
            "an injection-shaped border value must be skipped entirely"
        );
    }

    #[test]
    fn a_non_hex_border_inactive_is_skipped_while_a_valid_border_still_emits() {
        let notes = json!({
            "window": { "border": "#89b4fa", "borderInactive": "'; rm -rf ~; '" }
        });
        assert_eq!(
            geometry_keywords(&notes),
            vec!["keyword general:col.active_border rgb(89b4fa)".to_string()],
            "the valid sibling field still emits; only the malformed one is dropped"
        );
    }

    #[test]
    fn no_geometry_and_no_window_yields_an_empty_batch() {
        let notes = json!({ "palette": { "bg": "#1e1e2e" } });
        assert!(geometry_keywords(&notes).is_empty());
    }

    #[test]
    fn batch_command_joins_with_semicolons() {
        let kw = vec!["keyword a b".to_string(), "keyword c d".to_string()];
        assert_eq!(batch_command(&kw), "keyword a b; keyword c d");
    }

    #[test]
    fn apply_live_skips_with_no_keywords_without_touching_env() {
        assert_eq!(
            apply_live(&[]),
            "skipped (no geometry/border keywords resolved)"
        );
    }

    #[test]
    fn apply_live_skips_off_hyprland() {
        // Shares `aoide_test_support::env_lock()` with every other
        // env-touching test in the crate (rice.rs's own
        // `HYPRLAND_INSTANCE_SIGNATURE`-touching tests included) — this used
        // to lock a separate crate-local mutex, racing rice.rs tests that
        // touch the SAME env var under the other lock.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = aoide_test_support::EnvSaver::capture(&["HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");

        let kw = vec!["keyword general:gaps_out 8".to_string()];
        assert_eq!(apply_live(&kw), "skipped (HYPRLAND_INSTANCE_SIGNATURE unset)");
    }

    // ── terminal half ────────────────────────────────────────────────────

    /// A short scratch dir: a unix socket path must fit `sun_path` (108
    /// bytes), which a deep `$TMPDIR` can overrun on its own.
    fn short_tmp(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir = std::path::PathBuf::from(format!("/tmp/ak-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn terminal_colors_render_the_staged_base16_through_the_kitty_emitter() {
        let raw = std::fs::read_to_string("tests/fixtures/valid-base16.json").unwrap();
        let mut notes: Value = serde_json::from_str(&raw).unwrap();
        // A staged notes document carries the injected `"song"` and a
        // geometry block the resolver ignores.
        notes["song"] = json!("cadenza");
        notes["geometry"] = json!({ "blurEnabled": false });
        let out = terminal_colors(&notes).expect("valid notes resolve");
        assert!(out.lines().any(|l| l == "background #0a0a0d"), "{out}");
        assert!(out.lines().any(|l| l == "color15 #ffffff"), "{out}");
    }

    #[test]
    fn kitty_sockets_are_only_kitty_pid_named_unix_sockets() {
        let dir = short_tmp("socks");
        let _a = std::os::unix::net::UnixListener::bind(dir.join("kitty-123")).unwrap();
        let _b = std::os::unix::net::UnixListener::bind(dir.join("kitty-abc")).unwrap();
        let _c = std::os::unix::net::UnixListener::bind(dir.join("other-7")).unwrap();
        std::fs::write(dir.join("kitty-456"), "not a socket").unwrap();
        assert_eq!(kitty_sockets(&dir), vec![dir.join("kitty-123")]);
        assert!(kitty_sockets(&dir.join("absent")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_skips_quietly_without_a_runtime_dir_or_a_socket() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = aoide_test_support::EnvSaver::capture(&["XDG_RUNTIME_DIR"]);
        let conf = std::path::Path::new("/nonexistent/terminal-colors.conf");

        std::env::remove_var("XDG_RUNTIME_DIR");
        assert_eq!(push_kitty_colors(conf)["status"], "skipped");

        let dir = short_tmp("empty");
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        let out = push_kitty_colors(conf);
        assert_eq!(out["status"], "skipped");
        assert_eq!(out["message"], "no kitty control socket open");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_drives_set_colors_per_socket_and_counts_the_windows() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = aoide_test_support::EnvSaver::capture(&["XDG_RUNTIME_DIR", "PATH"]);
        let dir = short_tmp("push");
        let _l = std::os::unix::net::UnixListener::bind(dir.join("kitty-42")).unwrap();
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let log = dir.join("argv.log");
        // A stand-in `kitty`: records its argv, answers `ls` with two OS-window
        // tabs holding three windows between them.
        let shim = bin.join("kitty");
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\necho \"$*\" >> '{}'\ncase \"$*\" in *' ls') echo '[{{\"tabs\":[{{\"windows\":[{{}},{{}}]}},{{\"windows\":[{{}}]}}]}}]';; esac\n",
                log.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        std::env::set_var("PATH", &bin);

        let conf = dir.join("terminal-colors.conf");
        let out = push_kitty_colors(&conf);
        assert_eq!(out["status"], "applied", "{out}");
        assert_eq!(out["instances"], 1);
        assert_eq!(out["windows"], 3);
        let argv = std::fs::read_to_string(&log).unwrap();
        let sock = dir.join("kitty-42");
        assert_eq!(
            argv.lines().next().unwrap(),
            format!("@ --to unix:{} set-colors --all --configured {}", sock.display(), conf.display())
        );

        // No `kitty` on PATH at all: a quiet skip, never a failure.
        std::env::set_var("PATH", dir.join("nowhere"));
        assert_eq!(push_kitty_colors(&conf)["message"], "kitty not on PATH");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
