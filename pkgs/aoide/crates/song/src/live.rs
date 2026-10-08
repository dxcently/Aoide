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
//! terminal file (every colour slot through the livery engine's `kitty`
//! emitter, plus a `background_opacity` only when the song has an opinion)
//! and tells every open kitty over its control socket to reload its own
//! config, which includes that file. The
//! write itself stays with the caller (`commands::rice::stage_terminal_colors`),
//! like the stage file's: this module holds no storage dependency.

use crate::livery::schema;
use serde_json::Value;

/// The baked hyprglass switches, `(enabled, layers:enabled)`: what the
/// compositor lane's `plugin:hyprglass` block leaves in `hyprland.conf` for a
/// song with NO `geometry.blurEnabled` opinion — the block takes both keys
/// from that field, so an opinionated song's bake is glass off/on with it, the
/// same two values this crate stages. A staged song with no opinion restores
/// exactly this. Change it together with that block.
pub const HYPRGLASS_BAKED: (bool, bool) = (true, true);

/// Build the `hyprctl keyword …` list for one staged notes document, in the
/// fixed order CONTRACTS.md §1's geometry table lists them (gaps, border
/// size, border colours, rounding, blur), then the two hyprglass switches
/// (always emitted; see [`HYPRGLASS_BAKED`]).
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
    // compositor lane resolves for the aoide-* namespaces — the BAKE takes
    // both from that same `blurEnabled`, so a staged song agrees with a
    // desktop that booted straight into it.
    //
    // Unlike the geometry keywords above, these are ALWAYS emitted: a song
    // with no `blurEnabled` opinion restores the baked default
    // ([`HYPRGLASS_BAKED`]) rather than keeping whatever glass the previously
    // staged song left behind (house rule 10: a stage hot-loads the song as
    // declared, and a song that says nothing about glass is declared with the
    // baked glass). [`apply_live`] partitions them out and sends them as
    // their own second `hyprctl --batch`, so a host without the plugin loaded
    // loses its glass batch alone — the borders, gaps and blur keywords are
    // never entangled with the plugin's refusal.
    let blur = geo.and_then(|g| g.get("blurEnabled")).and_then(Value::as_bool);
    let (window_glass, layer_glass) = match blur {
        Some(b) => (b, b),
        None => HYPRGLASS_BAKED,
    };
    out.push(format!("keyword plugin:hyprglass:enabled {}", u8::from(window_glass)));
    out.push(format!("keyword plugin:hyprglass:layers:enabled {}", u8::from(layer_glass)));

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
///
/// TWO batches, deliberately: the `plugin:hyprglass:*` keywords go in their
/// own `hyprctl --batch` after the core one, so a host that never loaded the
/// plugin (a compositor without it, or a non-nix host) cannot fail the batch
/// the borders, gaps and blur share — and its refusal cannot be read as a
/// core-keyword failure. Off-Hyprland, both are skipped together, as before.
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
    // `cfg!(test)` is evaluated when THIS CRATE is compiled, so it protects
    // this crate's own unit tests and nothing else: a `lyra`/CLI integration
    // test, or any other crate linking this library, still reaches the
    // compositor when the operator's `HYPRLAND_INSTANCE_SIGNATURE` is set —
    // and every batch now carries the hyprglass switches, so such a run would
    // flip the live glass and borders. Nothing in the tree does that today.
    if cfg!(test) {
        return "skipped (this crate's own test build: no live hyprctl)";
    }

    let (core, glass) = partition_keywords(keywords);
    let core = if core.is_empty() { Batch::Applied } else { run_batch(&core) };
    let glass = if glass.is_empty() { Batch::Applied } else { run_batch(&glass) };
    live_status(core, glass)
}

/// Split a keyword list into the two batches [`apply_live`] runs, IN ORDER:
/// `(core, glass)`, where every `plugin:hyprglass:*` keyword lands in the
/// second one. That is the whole rule, stated once and unit-tested — an empty
/// side means "nothing to run for it".
///
/// The test is `geometry_keywords`' own output, not a hand-written list: the
/// emitted keywords carry the `keyword ` prefix (`keyword
/// plugin:hyprglass:enabled 0`), so a prefix match on the bare plugin name
/// would put both glass keywords in the CORE batch and the split would never
/// happen — a bug this predicate and that test exist to catch.
pub fn partition_keywords(keywords: &[String]) -> (Vec<String>, Vec<String>) {
    let mut core = Vec::new();
    let mut glass = Vec::new();
    for k in keywords {
        if k.contains("plugin:hyprglass:") {
            glass.push(k.clone());
        } else {
            core.push(k.clone());
        }
    }
    (core, glass)
}

/// The status string for a pair of batch outcomes — the whole
/// outcome→envelope mapping, pure so a test can reach it (`apply_live` itself
/// is inert in this crate's test builds). A failed GLASS batch is reported as
/// best-effort alongside a good core one rather than as a failure, because the
/// borders, gaps and blur are what a stage is judged on; a bad CORE batch
/// reports its own cause.
pub fn live_status(core: Batch, glass: Batch) -> &'static str {
    match (core, glass) {
        (Batch::Applied, Batch::Applied) => "applied",
        (Batch::Applied, _) => {
            "applied; best-effort: the hyprglass batch failed (plugin not loaded?) \
             — stage file already updated"
        }
        (Batch::Failed, _) => "best-effort: hyprctl reported an error (stage file already updated)",
        (Batch::Unavailable, _) => {
            "best-effort: hyprctl unavailable (stage file already updated)"
        }
    }
}

/// How one `hyprctl --batch` call went.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Batch {
    Applied,
    /// It ran and exited non-zero (an unknown keyword is one way).
    Failed,
    /// No `hyprctl` on `PATH`.
    Unavailable,
}

/// Run ONE `hyprctl --batch` payload. Never a `Result`: the caller only
/// reports which way it went.
fn run_batch(keywords: &[String]) -> Batch {
    match std::process::Command::new("hyprctl")
        .arg("--batch")
        .arg(batch_command(keywords))
        .output()
    {
        Ok(out) if out.status.success() => Batch::Applied,
        Ok(_) => Batch::Failed,
        Err(_) => Batch::Unavailable,
    }
}


// ── The terminal half ────────────────────────────────────────────────────
//
// kitty has no FileView: a colour file on disk reaches a NEW window through
// the kitty dendrite's `include` of it, and an OPEN window only when kitty is
// told. The telling goes through kitty's own control socket
// (`kitty @ load-config`, with no path, so kitty re-reads its own kitty.conf
// and every include), never a raw OSC write to a pty — an OSC write
// interleaves with whatever the program in that pty is printing and gets
// eaten. A reload resets every window to the configured state, so a song
// with no opinion lands on the host's bake with no value copied here. The
// sockets are found by the path pattern the kitty dendrite configures
// (`listen_on unix:${XDG_RUNTIME_DIR}/kitty-{kitty_pid}`), a plain directory
// listing — no process-table or /proc walk.

/// The staged terminal colour file's name inside the stage dir — the runtime
/// root's contract path `$AOIDE_ROOT/song/stage/terminal-colors.conf`, the
/// file the kitty dendrite includes.
pub const TERMINAL_COLORS_FILE: &str = "terminal-colors.conf";

/// How long one `kitty @` call may take before it is abandoned. A socket
/// whose kitty is wedged must not stall `rice stage`.
const KITTY_CALL_LIMIT: std::time::Duration = std::time::Duration::from_secs(3);

/// The song's terminal background opacity opinion: `geometry.terminalOpacity`
/// when it is a plain number in [0, 1]. `None` is no opinion (absent, null,
/// or a value lint would reject, which is never written): the host's bake
/// shows, because kitty falls through to its own config.
pub fn terminal_opacity(notes: &Value) -> Option<f64> {
    notes
        .get("geometry")
        .and_then(|g| g.get("terminalOpacity"))
        .and_then(schema::terminal_opacity_value)
}

/// Render the staged terminal file one notes document implies: the livery
/// engine's `kitty` emitter (every colour slot), then a `background_opacity`
/// line from [`terminal_opacity`] only for a song with an opinion. The file
/// is the last include of kitty.conf, so a line here beats the declared
/// fragment and the bake; without one kitty falls through to them. `None`
/// when the notes don't resolve (a torn or reference-cyclic notes file) — the
/// caller leaves the previous file in place rather than writing a guess.
pub fn terminal_colors(notes: &Value) -> Option<String> {
    let r = crate::livery::resolve(notes).ok()?;
    let mut out = crate::livery::emit::kitty::emit_kitty(&r);
    if let Some(o) = terminal_opacity(notes) {
        out.push_str(&format!("background_opacity {o}\n"));
    }
    Some(out)
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

/// Guarded, best-effort reload of every open kitty:
/// `kitty @ --to unix:<sock> load-config` per socket, with no path, so kitty
/// re-reads its own kitty.conf and every include (the staged terminal file
/// among them) and an open window ends where a new one opens; then an `ls` of
/// the instance to count its windows. kitty moves `background_opacity` on a
/// reload only for an instance started with `dynamic_background_opacity yes`,
/// which both kitty lanes set. Every window's font zoom resets to the
/// configured size. Never a `Result` — the same tier as [`apply_live`]: the
/// file on disk is already the truth for every new kitty, and this only
/// brings the open ones along.
///
/// A quiet skip, not a failure, when there is nothing to reload: no
/// `$XDG_RUNTIME_DIR`, no `kitty-<pid>` socket under it, or no `kitty` on
/// `PATH`. The returned object is the outcome envelope's `terminal` data:
/// `{status, message, instances, windows, failed}`.
pub fn reload_kitty() -> Value {
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
        match kitty_call(&["@".as_ref(), "--to".as_ref(), to.as_os_str(), "load-config".as_ref()]) {
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
            "reloaded {windows} kitty window(s) across {instances} instance(s){}",
            if failed > 0 { format!("; {failed} socket(s) did not answer") } else { String::new() },
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

    /// The two hyprglass lines a song with no `blurEnabled` opinion gets:
    /// the baked default, closing every batch.
    fn baked_glass() -> Vec<String> {
        let (w, l) = HYPRGLASS_BAKED;
        vec![
            format!("keyword plugin:hyprglass:enabled {}", u8::from(w)),
            format!("keyword plugin:hyprglass:layers:enabled {}", u8::from(l)),
        ]
    }

    fn with_baked_glass(mut v: Vec<String>) -> Vec<String> {
        v.extend(baked_glass());
        v
    }

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
    fn hyprglass_follows_blur_and_restores_the_baked_glass_without_an_opinion() {
        // cadenza's shape: blur off, glass off, both live keywords.
        let off = geometry_keywords(&json!({ "geometry": { "blurEnabled": false, "rounding": 0 } }));
        assert!(off.contains(&"keyword plugin:hyprglass:enabled 0".to_string()));
        assert!(off.contains(&"keyword plugin:hyprglass:layers:enabled 0".to_string()));
        // A song that wants blur turns the glass back on.
        let on = geometry_keywords(&json!({ "geometry": { "blurEnabled": true } }));
        assert!(on.contains(&"keyword plugin:hyprglass:enabled 1".to_string()));
        assert!(on.contains(&"keyword plugin:hyprglass:layers:enabled 1".to_string()));
        // No opinion (null, missing field, no geometry block at all) restores
        // the baked glass, so a blur-off song's glass never outlives it.
        for notes in [
            json!({ "geometry": { "blurEnabled": null } }),
            json!({ "geometry": { "rounding": 4 } }),
            json!({}),
        ] {
            let kw = geometry_keywords(&notes);
            assert_eq!(kw[kw.len() - 2..].to_vec(), baked_glass(), "{notes}");
            // Hyprland's own blur keeps the plain no-opinion rule.
            assert!(kw.iter().all(|k| !k.contains("decoration:blur")), "{kw:?}");
        }
        assert_eq!(HYPRGLASS_BAKED, (true, true), "the compositor lane bakes both on");
    }

    #[test]
    fn no_geometry_block_still_emits_the_always_resolved_border_colours() {
        let notes = json!({ "window": { "border": "#a07414", "borderInactive": "#3f867e" } });
        assert_eq!(
            geometry_keywords(&notes),
            with_baked_glass(vec![
                "keyword general:col.active_border rgb(a07414)".to_string(),
                "keyword general:col.inactive_border rgb(3f867e)".to_string(),
            ])
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
            with_baked_glass(vec!["keyword general:gaps_in 6".to_string()])
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
        assert_eq!(
            geometry_keywords(&notes),
            baked_glass(),
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
            with_baked_glass(vec!["keyword general:col.active_border rgb(89b4fa)".to_string()]),
            "the valid sibling field still emits; only the malformed one is dropped"
        );
    }

    #[test]
    fn no_geometry_and_no_window_yields_only_the_baked_glass() {
        let notes = json!({ "palette": { "bg": "#1e1e2e" } });
        assert_eq!(geometry_keywords(&notes), baked_glass());
    }

    #[test]
    fn batch_command_joins_with_semicolons() {
        let kw = vec!["keyword a b".to_string(), "keyword c d".to_string()];
        assert_eq!(batch_command(&kw), "keyword a b; keyword c d");
    }

    // The two-batch split (F5) and its status mapping, reached directly —
    // `apply_live` is inert in this crate's own test builds, so these pure
    // halves are the only way to pin the behaviour they implement.
    #[test]
    fn partition_keywords_sends_the_glass_pair_second_and_keeps_order() {
        let kw = vec![
            "keyword general:gaps_out 8".to_string(),
            "keyword plugin:hyprglass:enabled 0".to_string(),
            "keyword general:border_size 1".to_string(),
            "keyword plugin:hyprglass:layers:enabled 0".to_string(),
        ];
        let (core, glass) = partition_keywords(&kw);
        assert_eq!(
            core,
            vec![
                "keyword general:gaps_out 8".to_string(),
                "keyword general:border_size 1".to_string(),
            ]
        );
        assert_eq!(
            glass,
            vec![
                "keyword plugin:hyprglass:enabled 0".to_string(),
                "keyword plugin:hyprglass:layers:enabled 0".to_string(),
            ]
        );
        // Every partition is complete and disjoint — nothing is dropped.
        assert_eq!(core.len() + glass.len(), kw.len());
        // A list with no glass keywords leaves the second batch empty.
        assert_eq!(partition_keywords(&core).1, Vec::<String>::new());
    }

    /// The partition against the REAL emitted list — the assertion that
    /// catches a predicate which never matches (`keyword plugin:hyprglass:…`
    /// carries the `keyword ` prefix).
    #[test]
    fn every_glass_keyword_the_emitter_produces_lands_in_the_glass_batch() {
        let kw = geometry_keywords(&json!({ "geometry": { "blurEnabled": false, "rounding": 0 } }));
        let (core, glass) = partition_keywords(&kw);
        assert_eq!(glass.len(), 2, "both hyprglass keys, and only those: {glass:?}");
        assert!(
            glass.iter().all(|k| k.contains("plugin:hyprglass:")),
            "{glass:?}"
        );
        assert!(
            core.iter().all(|k| !k.contains("plugin:hyprglass:")),
            "the core batch must hold no glass keyword: {core:?}"
        );
        assert!(!core.is_empty(), "and it must not be empty for a real song");
    }

    #[test]
    fn live_status_covers_every_outcome_pair() {
        assert_eq!(live_status(Batch::Applied, Batch::Applied), "applied");
        // A failed glass batch (or an absent `hyprctl` FOR it) never hides a
        // good core batch: the borders and gaps did apply.
        for glass in [Batch::Failed, Batch::Unavailable] {
            assert_eq!(
                live_status(Batch::Applied, glass),
                "applied; best-effort: the hyprglass batch failed (plugin not loaded?) \
                 — stage file already updated"
            );
        }
        assert_eq!(
            live_status(Batch::Failed, Batch::Applied),
            "best-effort: hyprctl reported an error (stage file already updated)"
        );
        assert_eq!(
            live_status(Batch::Unavailable, Batch::Applied),
            "best-effort: hyprctl unavailable (stage file already updated)"
        );
        // The core failure wins the report even when the glass batch also
        // failed — there is one string, and the core keyword is the reason.
        assert_eq!(
            live_status(Batch::Failed, Batch::Failed),
            "best-effort: hyprctl reported an error (stage file already updated)"
        );
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
        assert!(!out.contains("background_opacity"), "no opinion writes no line: {out}");
    }

    #[test]
    fn terminal_opacity_is_the_songs_opinion_or_no_line() {
        let raw = std::fs::read_to_string("tests/fixtures/valid-base16.json").unwrap();
        let mut notes: Value = serde_json::from_str(&raw).unwrap();
        for (geometry, want) in [
            (json!({ "terminalOpacity": 0.7 }), Some("0.7")),
            (json!({ "terminalOpacity": 1 }), Some("1")),
            (json!({ "terminalOpacity": 0 }), Some("0")),
            (json!({ "terminalOpacity": null }), None),
            (json!({}), None),
            (Value::Null, None),
            // Lint rejects these; the hot path treats them as no opinion.
            (json!({ "terminalOpacity": 1.5 }), None),
            (json!({ "terminalOpacity": "0.7\nshell /bin/evil" }), None),
        ] {
            notes["geometry"] = geometry.clone();
            let out = terminal_colors(&notes).unwrap();
            let lines: Vec<&str> =
                out.lines().filter(|l| l.starts_with("background_opacity")).collect();
            let want: Vec<String> = want.iter().map(|w| format!("background_opacity {w}")).collect();
            assert_eq!(lines, want, "{geometry}");
            assert!(!out.contains("evil"));
        }
        notes.as_object_mut().unwrap().remove("geometry");
        assert!(!terminal_colors(&notes).unwrap().contains("background_opacity"));
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
    fn reload_skips_quietly_without_a_runtime_dir_or_a_socket() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = aoide_test_support::EnvSaver::capture(&["XDG_RUNTIME_DIR"]);

        std::env::remove_var("XDG_RUNTIME_DIR");
        assert_eq!(reload_kitty()["status"], "skipped");

        let dir = short_tmp("empty");
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        let out = reload_kitty();
        assert_eq!(out["status"], "skipped");
        assert_eq!(out["message"], "no kitty control socket open");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reload_drives_load_config_per_socket_and_counts_the_windows() {
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

        let out = reload_kitty();
        assert_eq!(out["status"], "applied", "{out}");
        assert_eq!(out["instances"], 1);
        assert_eq!(out["windows"], 3);
        assert_eq!(out["failed"], 0);
        assert!(out.get("opacity").is_none() && out.get("opacity_refused").is_none(), "{out}");
        let argv = std::fs::read_to_string(&log).unwrap();
        let sock = dir.join("kitty-42");
        assert_eq!(
            argv.lines().collect::<Vec<_>>(),
            vec![
                format!("@ --to unix:{} load-config", sock.display()),
                format!("@ --to unix:{} ls", sock.display()),
            ]
        );
        assert!(!argv.contains("set-colors") && !argv.contains("set-background-opacity"));

        // A kitty that refuses the reload counts as failed, and nothing applied.
        std::fs::write(&shim, "#!/bin/sh\ncase \"$*\" in *load-config*) exit 1;; esac\n").unwrap();
        let out = reload_kitty();
        assert_eq!(out["status"], "failed", "{out}");
        assert_eq!(out["failed"], 1);
        assert_eq!(out["instances"], 0);

        // No `kitty` on PATH at all: a quiet skip, never a failure.
        std::env::set_var("PATH", dir.join("nowhere"));
        assert_eq!(reload_kitty()["message"], "kitty not on PATH");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
