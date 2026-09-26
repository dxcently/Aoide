//! `lyra onboard` — the nix half of the onboarding flow (docs/architecture/
//! ONBOARD.md), reached only by `aoide onboard`'s delegate spawn
//! (`crates/cli/src/commands/onboard.rs::probe_and_delegate_lyra`) once
//! `rice_bin()` resolves. Derives every `aoide.*` option the modules
//! declare (`flake.nix`'s `aoideOptions` output, `lib/options.nix` — ladder
//! rung (a), `lib.evalModules` + `lib.optionAttrSetToDocList`, held with no
//! fallback to `nixosSystem` needed) and renders `aoide.nix`: a nix module
//! the user imports, every option commented out at its current default, so
//! importing the file with nothing uncommented changes nothing (decisions
//! 2/5/6). NEVER touches the user's flake — the teaching is the printed
//! `imports = [ … ];` line, not an edit.
//!
//! Door::Cli-only (ONBOARD.md's flow diagram): an interactive install flow
//! that shells out to `nix eval` makes no sense over MCP/A2A/the daemon,
//! the same reasoning core's own `onboard` carries. From a checkout only
//! (decision 10): the derivation needs `modules/` and `flake.nix`, which a
//! built/installed `lyra` never carries — [`checkout_root`] is lyra's own
//! equivalent of core's `hooks::skill_source()` walk-up, but anchored on
//! different markers: lyra has no skill dir to walk up to, and
//! `current_exe()` is the WRONG anchor here (a built `lyra` binary lives in
//! the nix store, nowhere near the checkout that spawned it) — a plain
//! `cwd` walk-up looking for `flake.nix` + `pkgs/aoide` is the cleanest
//! equivalent, and correct in practice: core's own `checkout_root()` already
//! proved the invoking cwd sits inside the checkout before it ever spawns
//! `lyra onboard` (inherited stdio, inherited cwd — Command::new never
//! changes it), so lyra's independent walk-up finds the exact same root.

use crate::dispatch::Invocation;
use crate::output::Outcome;
use crate::registry::{cmd, flag, Registry};
use aoide_protocol::Door;
use serde_json::json;
use std::path::{Path, PathBuf};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["onboard"],
        summary: "Generate `aoide.nix`: every `aoide.*` module option, derived fresh from modules/{nucleus,dendrites}, commented out at its current default -- then print the `imports` line to teach it in. Never edits the user's flake.",
        args: [],
        flags: [
            flag!("out", "string", "Where to emit the generated module (default: ./aoide.nix)."),
            flag!("yes", "bool", "Skip the interactive regenerate confirm on a rerun over a previously-generated file; warn on stderr and proceed instead."),
        ],
        gated: false,
        implemented: true,
        handler: handle_onboard,
    ));
}

fn handle_onboard(inv: &Invocation) -> Outcome {
    let cmd = "onboard";
    if inv.door != Door::Cli {
        return Outcome::usage(
            cmd,
            "onboard is an interactive install flow; run `lyra onboard` from a terminal (not over this door)",
        );
    }

    let options = match read_aoide_options() {
        Ok(o) => o,
        Err(e) => return Outcome::error(cmd, format!("deriving the aoide.* option set: {e}")),
    };

    let out_path = PathBuf::from(inv.flags.get("out").map(String::as_str).unwrap_or("./aoide.nix"));
    let door = inv.door;
    let yes = inv.flag_present("yes");

    match emit_generated(&out_path, &options, || resolve_regenerate(&out_path, door, yes)) {
        Ok(r) => {
            let teach = format!("imports = [ {} ];", out_path.display());
            println!("{teach}");
            let mut changed = vec![r.wrote.display().to_string()];
            if let Some(bak) = &r.backed_up {
                changed.push(bak.display().to_string());
            }
            let message = if r.regenerated {
                format!("lyra onboard: regenerated {} ({} options) -- {teach}", out_path.display(), options.len())
            } else {
                format!("lyra onboard: wrote {} ({} options) -- {teach}", out_path.display(), options.len())
            };
            Outcome::ok(cmd, message).changed(changed).with_data(json!({
                "out": out_path.display().to_string(),
                "optionCount": options.len(),
                "backedUp": r.backed_up.map(|p| p.display().to_string()),
                "importsLine": teach,
            }))
        }
        Err(e) => Outcome::error(cmd, e),
    }
}

/// The invoking checkout's root: walk up from `cwd` looking for a directory
/// carrying both `flake.nix` and `pkgs/aoide` — the two markers that
/// distinguish a real Aoide checkout from an arbitrary directory. See the
/// The `aoide.*` option doc set, read from the SHIPPED
/// `share/lyra/aoide-options.json` — `pkgs/lyra-songbook` builds it from
/// `lib/options.nix` at nix-build time, and it sits beside the song templates
/// [`aoide_storage::fs::lyra_share_dir`] resolves. The FILE is the eval, so
/// `lyra onboard` needs neither a checkout nor `nix` on PATH: it works from any
/// cwd, on a machine that has never seen the Aoide source.
fn read_aoide_options() -> Result<Vec<OptionEntry>, String> {
    let share = aoide_storage::fs::lyra_share_dir()
        .ok_or_else(|| "no shipped lyra data dir: $AOIDE_SONG_TEMPLATES names none (or is not absolute) and no `share/lyra/songbook` sits beside this binary".to_string())?;
    let path = share.join("aoide-options.json");
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("cannot read the shipped option set at {} ({e})", path.display()))?;
    parse_options(&bytes)
}

/// One derived `aoide.*` option, narrowed to exactly what the formatter
/// needs — the same three fields `lib/options.nix` emits. `default` is
/// already-rendered nix SOURCE TEXT (nixpkgs' own doc renderer did the
/// quoting/escaping; `render_aoide_nix` pastes it verbatim), `None` when the
/// option has no static default (nixpkgs' doc list omits the `default` key
/// entirely in that case — the 16 `aoide.livery.base16.*` slots and
/// `aoide.surfaces.<name>.owner` today).
#[derive(Debug)]
struct OptionEntry {
    name: String,
    description: String,
    default: Option<String>,
}

/// Parse [`read_aoide_options`]'s payload — a flat JSON array of `{name,
/// description, default?}` objects, `lib/options.nix`'s exact output shape.
/// Sorted by `name` on the way out so the generated file's ordering is
/// deterministic regardless of the order the eval emitted.
fn parse_options(bytes: &[u8]) -> Result<Vec<OptionEntry>, String> {
    let parsed: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("aoide options eval output isn't valid JSON: {e}"))?;
    let arr = parsed.as_array().ok_or_else(|| "aoide options eval output is not a JSON array".to_string())?;

    let mut out = Vec::with_capacity(arr.len());
    for entry in arr {
        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("option entry missing a string `name`: {entry}"))?
            .to_string();
        let description = entry.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let default = entry.get("default").and_then(|v| v.as_str()).map(str::to_string);
        out.push(OptionEntry { name, description, default });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The header line marking a file as generated by this command — the ONLY
/// "ours" detector [`wiring_state`] uses (decision 3's "already-configured"
/// outcome). Deliberately names no specific `.bak` path (a custom `--out`
/// backs up next to itself, not to a hardcoded `aoide.nix.bak`) so the text
/// stays true regardless of where a given file landed.
const GENERATED_HEADER: &str =
    "# Generated by `lyra onboard` -- re-running this command regenerates this file in place (the previous version is copied to a .bak file first, overwritten on the next rerun).";

/// The three states a `--out` target can be in (ONBOARD.md decision 3, minus
/// "skip" — core never spawns `lyra onboard` at all when lyra is absent, so
/// that outcome never reaches this crate).
enum FileState {
    /// Nothing at the path yet — the plain "configure" case.
    Absent,
    /// A file this generator wrote on a previous run (its first line is
    /// [`GENERATED_HEADER`]) — the "already-configured" case (decision 8:
    /// warn, back up, regenerate).
    Ours,
    /// A file exists but was not generated by this command — never touched
    /// (decision 2's "emit and teach, never edit").
    Foreign,
}

fn wiring_state(path: &Path) -> FileState {
    if std::fs::symlink_metadata(path).is_err() {
        return FileState::Absent;
    }
    match std::fs::read_to_string(path) {
        Ok(content) if content.lines().next().map(str::trim) == Some(GENERATED_HEADER) => FileState::Ours,
        _ => FileState::Foreign,
    }
}

/// A successful [`emit_generated`] write.
#[derive(Debug)]
struct EmitResult {
    /// The path actually written — always `out_path`.
    wrote: PathBuf,
    /// The backup path, when a previous generation was overwritten.
    backed_up: Option<PathBuf>,
    /// Whether this was a rerun-over-ours (vs. a fresh emit) — only changes
    /// the caller's own message wording.
    regenerated: bool,
}

/// Emit `aoide.nix` at `out_path`, given ALREADY-DERIVED `options` — the
/// nix-free half of the flow, and the exact seam the "rerun semantics"
/// tests drive directly (never through a real `nix eval`). `allow_regenerate`
/// is called ONLY in the [`FileState::Ours`] branch, exactly once, and never
/// at all for [`FileState::Absent`]/[`FileState::Foreign`] — the injection
/// point for decision 8's warn-then-confirm-or-proceed choice, which is
/// itself impure (reads a tty or stdin) and therefore lives in the caller
/// ([`resolve_regenerate`]), not here.
fn emit_generated(
    out_path: &Path,
    options: &[OptionEntry],
    allow_regenerate: impl FnOnce() -> bool,
) -> Result<EmitResult, String> {
    match wiring_state(out_path) {
        FileState::Foreign => Err(format!(
            "{} already exists and was not generated by `lyra onboard` -- move it aside or pass a different --out (never overwriting a hand-written file)",
            out_path.display()
        )),
        FileState::Ours => {
            if !allow_regenerate() {
                return Err(format!("regeneration declined -- {} left untouched", out_path.display()));
            }
            let bak = PathBuf::from(format!("{}.bak", out_path.display()));
            std::fs::copy(out_path, &bak)
                .map_err(|e| format!("failed to back up {} -> {}: {e}", out_path.display(), bak.display()))?;
            std::fs::write(out_path, render_aoide_nix(options))
                .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
            Ok(EmitResult { wrote: out_path.to_path_buf(), backed_up: Some(bak), regenerated: true })
        }
        FileState::Absent => {
            if let Some(parent) = out_path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
                }
            }
            std::fs::write(out_path, render_aoide_nix(options))
                .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
            Ok(EmitResult { wrote: out_path.to_path_buf(), backed_up: None, regenerated: false })
        }
    }
}

/// Decision 8's warn/confirm choice, called only when [`wiring_state`] found
/// our own previous header. On a tty without `--yes` this is a real
/// `protocol::pick::confirm` prompt; with `--yes` or off a tty, it warns on
/// stderr and proceeds without ever touching stdin — both escape hatches
/// ONBOARD.md's prompt substrate section calls load-bearing (agents run the
/// identical flow non-interactively). Not unit-tested for the tty branch —
/// same "exercised by hand" status `protocol::pick`'s own module doc gives
/// its `inquire`-backed paths.
fn resolve_regenerate(out_path: &Path, door: Door, yes: bool) -> bool {
    if aoide_protocol::interactive(door) && !yes {
        aoide_protocol::pick::confirm(&format!("regenerate? old file -> {}.bak", out_path.display())).unwrap_or(false)
    } else {
        eprintln!(
            "lyra onboard: {} already exists (generated by a previous run) -- backing it up to {}.bak and regenerating",
            out_path.display(),
            out_path.display()
        );
        true
    }
}

/// Collapse a possibly-multi-paragraph option description to ONE line
/// (decision 2/5's "one-line description comment" — reading top to bottom
/// IS the teaching, so a paragraph-long description would defeat that).
/// Newlines collapse to single spaces first (so a sentence that wraps across
/// the source's own line breaks still joins correctly), then only the first
/// sentence (up to, not including, the first `". "`) survives.
fn first_line(description: &str) -> String {
    let collapsed = description.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.split_once(". ") {
        Some((first, _)) => format!("{first}."),
        None => collapsed,
    }
}

/// The env-knob appendix (decision 5) — the ONE allowed hand-list, since env
/// vars are not module options and so cannot be derived. Kept small; each
/// entry notes where it's documented/read.
const ENV_KNOBS: &[(&str, &str)] = &[
    (
        "AOIDE_CONDUCT_AUTOGATE",
        "{1,true,yes,all} auto-approves `graph send`'s gate (crates/conduct/src/graph/send.rs).",
    ),
    (
        "AOIDE_TERMINAL",
        "Windowed-conduct terminal invocation template, e.g. \"kitty -e {cmd}\" (crates/conduct/src/graph/spawn.rs).",
    ),
    (
        "AOIDE_CORE_BIN / AOIDE_RICE_BIN",
        "Override the sibling-binary resolver for aoide/lyra cross-exec (crates/protocol/src/bin.rs).",
    ),
    (
        "AOIDE_DISCOVERY_ADVERTISE",
        "Truthy forces the A2A LAN discovery advertisement on; mirrors aoide.a2a.discoveryAdvertise, beside the runtime `aoide node advertise on|off` switch (docs/architecture/PAIRING.md).",
    ),
];

/// The pure formatter: `options` in, the whole `aoide.nix` text out. A real
/// nix module, `{ config, lib, pkgs, ... }: { … }` — the function head is
/// required, not decorative: several derived defaults are literal nix
/// SOURCE TEXT that references `config` (`aoide.root`/`aoide.checkout`'s
/// `"/home/${config.aoide.user}/…"`, `aoide.auditLog`'s `config.aoide.root`
/// chain, the melete/mneme path defaults) — pasting that text into a
/// bare attrset with no `config` in scope would fail to evaluate the moment
/// a user uncomments one of those lines. Every option renders as ONE dotted
/// attrpath assignment (`aoide.a2a.port = 8710;`) — nix accepts a dotted
/// path directly as a binding, so no manual nesting is needed to mirror the
/// real `aoide.*` structure.
fn render_aoide_nix(options: &[OptionEntry]) -> String {
    let mut out = String::new();
    out.push_str(GENERATED_HEADER);
    out.push('\n');
    out.push_str("#\n");
    out.push_str("# Every `aoide.*` option the modules declare, derived fresh each run --\n");
    out.push_str("# never a hand-list. Each line below is commented out at its current\n");
    out.push_str("# default; importing this file with nothing uncommented changes nothing.\n");
    out.push_str("# Uncomment a line to set that option.\n");
    out.push('\n');
    out.push_str("{ config, lib, pkgs, ... }:\n");
    out.push_str("{\n");

    for opt in options {
        out.push_str("  # ");
        out.push_str(&first_line(&opt.description));
        out.push('\n');
        match &opt.default {
            Some(default) => out.push_str(&format!("  # {} = {default};\n", opt.name)),
            None => out.push_str(&format!("  # {} = <no static default -- see description>;\n", opt.name)),
        }
        out.push('\n');
    }

    out.push_str("}\n");

    out.push('\n');
    out.push_str("# ── Env knobs (not module options -- read at runtime) ──────────────────\n");
    for (name, note) in ENV_KNOBS {
        out.push_str(&format!("# {name}: {note}\n"));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{env_lock, unique_tmp, EnvSaver};

    // ── read_aoide_options: the shipped option set, no nix, no checkout ──

    #[test]
    fn read_aoide_options_reads_the_shipped_file_beside_the_song_templates() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_SONG_TEMPLATES", "PATH"]);
        let root = unique_tmp("lyra-onboard-shipped-options");
        let share = root.join("share").join("lyra");
        std::fs::create_dir_all(share.join("songbook")).unwrap();
        std::fs::write(
            share.join("aoide-options.json"),
            r#"[{"name":"aoide.livery.base16.base00","description":"base00"},
                {"name":"aoide.a2a.port","description":"The A2A HTTP port.","default":"8710"}]"#,
        )
        .unwrap();
        // No `nix` on PATH at all: the shipped file IS the eval, and this
        // proves the command cannot be reaching for one.
        let empty = root.join("empty-bin");
        std::fs::create_dir_all(&empty).unwrap();
        std::env::set_var("PATH", &empty);
        std::env::set_var("AOIDE_SONG_TEMPLATES", share.join("songbook"));

        let options = read_aoide_options().expect("the shipped option set reads with no nix");
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].name, "aoide.a2a.port", "sorted by name");
        assert_eq!(options[0].default.as_deref(), Some("8710"));
        assert_eq!(
            options[1].default, None,
            "a doc entry with no static default stays None"
        );

        let rendered = render_aoide_nix(&options);
        assert!(rendered.contains("  # aoide.a2a.port = 8710;\n"), "{rendered}");
        assert!(
            rendered
                .contains("  # aoide.livery.base16.base00 = <no static default -- see description>;\n"),
            "{rendered}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn read_aoide_options_teaches_when_no_shipped_data_dir_resolves() {
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_SONG_TEMPLATES"]);
        std::env::remove_var("AOIDE_SONG_TEMPLATES");

        let err = read_aoide_options().expect_err("no shipped dir, no options");
        assert!(err.contains("AOIDE_SONG_TEMPLATES"), "{err}");
        assert!(err.contains("share/lyra/songbook"), "{err}");
    }

    // ── parse_options: the eval payload shape ────────────────────────────

    #[test]
    fn parse_options_reads_name_description_and_default() {
        let json = br#"[{"name":"aoide.a2a.port","description":"The A2A HTTP port.","default":"8710"}]"#;
        let opts = parse_options(json).unwrap();
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].name, "aoide.a2a.port");
        assert_eq!(opts[0].description, "The A2A HTTP port.");
        assert_eq!(opts[0].default.as_deref(), Some("8710"));
    }

    #[test]
    fn parse_options_treats_a_missing_default_key_as_none() {
        let json = br#"[{"name":"aoide.livery.base16.base00","description":"base16 base00: default background."}]"#;
        let opts = parse_options(json).unwrap();
        assert_eq!(opts[0].default, None);
    }

    #[test]
    fn parse_options_sorts_by_name() {
        let json = br#"[{"name":"aoide.z","description":""},{"name":"aoide.a","description":""}]"#;
        let opts = parse_options(json).unwrap();
        assert_eq!(opts.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), vec!["aoide.a", "aoide.z"]);
    }

    #[test]
    fn parse_options_rejects_a_non_array_top_level() {
        let err = parse_options(br#"{"name":"aoide.a"}"#).unwrap_err();
        assert!(err.contains("not a JSON array"), "error: {err}");
    }

    #[test]
    fn parse_options_rejects_an_entry_missing_name() {
        let err = parse_options(br#"[{"description":"no name here"}]"#).unwrap_err();
        assert!(err.contains("missing a string `name`"), "error: {err}");
    }

    #[test]
    fn parse_options_rejects_invalid_json() {
        let err = parse_options(b"not json").unwrap_err();
        assert!(err.contains("isn't valid JSON"), "error: {err}");
    }

    // ── first_line: description collapse ─────────────────────────────────

    #[test]
    fn first_line_takes_the_first_sentence_of_a_multi_sentence_description() {
        assert_eq!(first_line("First sentence. Second sentence, more detail."), "First sentence.");
    }

    #[test]
    fn first_line_collapses_embedded_newlines_to_spaces() {
        assert_eq!(first_line("Line one\nstill line one.\nLine two never appears."), "Line one still line one.");
    }

    #[test]
    fn first_line_returns_the_whole_text_when_there_is_no_sentence_break() {
        assert_eq!(first_line("Just one clause, no period"), "Just one clause, no period");
    }

    // ── render_aoide_nix: the pure formatter, golden-style ────────────────

    fn opt(name: &str, description: &str, default: Option<&str>) -> OptionEntry {
        OptionEntry { name: name.to_string(), description: description.to_string(), default: default.map(str::to_string) }
    }

    #[test]
    fn render_starts_with_the_exact_generated_header() {
        let text = render_aoide_nix(&[]);
        assert_eq!(text.lines().next(), Some(GENERATED_HEADER));
    }

    #[test]
    fn render_emits_a_real_module_function_head_for_config_referencing_defaults() {
        let text = render_aoide_nix(&[]);
        assert!(text.contains("{ config, lib, pkgs, ... }:\n"), "text:\n{text}");
    }

    #[test]
    fn render_writes_a_commented_dotted_assignment_with_its_literal_default() {
        let text = render_aoide_nix(&[opt("aoide.a2a.port", "The A2A HTTP port.", Some("8710"))]);
        assert!(text.contains("  # The A2A HTTP port.\n  # aoide.a2a.port = 8710;\n"), "text:\n{text}");
    }

    #[test]
    fn render_pastes_a_config_referencing_default_verbatim() {
        let text = render_aoide_nix(&[opt(
            "aoide.auditLog",
            "Path to the single audit log.",
            Some(r#""/home/${config.aoide.user}/Aoide/log""#),
        )]);
        assert!(
            text.contains(r#"  # aoide.auditLog = "/home/${config.aoide.user}/Aoide/log";"#),
            "text:\n{text}"
        );
    }

    #[test]
    fn render_marks_a_no_static_default_option_explicitly() {
        let text = render_aoide_nix(&[opt("aoide.livery.base16.base00", "base16 base00: default background.", None)]);
        assert!(
            text.contains("  # aoide.livery.base16.base00 = <no static default -- see description>;\n"),
            "text:\n{text}"
        );
    }

    #[test]
    fn render_uses_only_the_first_sentence_of_a_long_description() {
        let text = render_aoide_nix(&[opt("aoide.x", "First sentence here. Second sentence with much more detail.", Some("1"))]);
        assert!(text.contains("  # First sentence here.\n"), "text:\n{text}");
        assert!(!text.contains("Second sentence"), "text:\n{text}");
    }

    #[test]
    fn render_carries_every_env_knob_in_the_appendix() {
        let text = render_aoide_nix(&[]);
        for (name, _) in ENV_KNOBS {
            assert!(text.contains(name), "missing env knob {name} in:\n{text}");
        }
    }

    // ── wiring_state / emit_generated: rerun semantics (scratch-dir, no nix) ─

    #[test]
    fn wiring_state_is_absent_for_a_missing_path() {
        let root = unique_tmp("lyra-onboard-wiring-absent");
        std::fs::create_dir_all(&root).unwrap();
        assert!(matches!(wiring_state(&root.join("aoide.nix")), FileState::Absent));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wiring_state_is_ours_for_a_file_carrying_the_generated_header() {
        let root = unique_tmp("lyra-onboard-wiring-ours");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        std::fs::write(&path, format!("{GENERATED_HEADER}\n{{ }}\n")).unwrap();
        assert!(matches!(wiring_state(&path), FileState::Ours));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wiring_state_is_foreign_for_a_hand_written_file() {
        let root = unique_tmp("lyra-onboard-wiring-foreign");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        std::fs::write(&path, "{ aoide.a2a.enable = true; }\n").unwrap();
        assert!(matches!(wiring_state(&path), FileState::Foreign));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn emit_generated_writes_fresh_with_no_backup() {
        let root = unique_tmp("lyra-onboard-emit-fresh");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");

        let result = emit_generated(&path, &[opt("aoide.a2a.port", "port", Some("8710"))], || {
            panic!("allow_regenerate must never be called on a fresh emit")
        })
        .unwrap();

        assert!(result.backed_up.is_none());
        assert!(!result.regenerated);
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().next(), Some(GENERATED_HEADER));
        assert!(content.contains("aoide.a2a.port = 8710;"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn emit_generated_backs_up_and_regenerates_when_allowed() {
        let root = unique_tmp("lyra-onboard-emit-rerun");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        emit_generated(&path, &[opt("aoide.a", "a", Some("1"))], || false).unwrap();
        let first_content = std::fs::read_to_string(&path).unwrap();

        let result = emit_generated(&path, &[opt("aoide.b", "b", Some("2"))], || true).unwrap();

        assert!(result.regenerated);
        let bak = result.backed_up.expect("a rerun over ours must back up");
        assert_eq!(bak, PathBuf::from(format!("{}.bak", path.display())));
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), first_content, "the .bak holds the PREVIOUS generation");
        let new_content = std::fs::read_to_string(&path).unwrap();
        assert!(new_content.contains("aoide.b = 2;"));
        assert!(!new_content.contains("aoide.a = 1;"), "the old option must not survive into the regenerated file");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn emit_generated_declines_without_writing_when_regenerate_is_refused() {
        let root = unique_tmp("lyra-onboard-emit-declined");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        emit_generated(&path, &[opt("aoide.a", "a", Some("1"))], || false).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let err = emit_generated(&path, &[opt("aoide.b", "b", Some("2"))], || false).unwrap_err();

        assert!(err.contains("regeneration declined"), "error: {err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before, "a declined regenerate must leave the file untouched");
        assert!(!root.join("aoide.nix.bak").exists(), "a declined regenerate must never write a backup");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn emit_generated_refuses_a_foreign_file_and_never_calls_allow_regenerate() {
        let root = unique_tmp("lyra-onboard-emit-foreign");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        std::fs::write(&path, "{ aoide.a2a.enable = true; }\n").unwrap();

        let err = emit_generated(&path, &[opt("aoide.b", "b", Some("2"))], || {
            panic!("allow_regenerate must never be called for a foreign file")
        })
        .unwrap_err();

        assert!(err.contains("not generated by `lyra onboard`"), "error: {err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ aoide.a2a.enable = true; }\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn emit_generated_bak_is_a_single_file_overwritten_each_rerun() {
        let root = unique_tmp("lyra-onboard-emit-bak-overwrite");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        emit_generated(&path, &[opt("aoide.a", "a", Some("1"))], || false).unwrap();
        let r2 = emit_generated(&path, &[opt("aoide.b", "b", Some("2"))], || true).unwrap();
        let bak_after_first_rerun = std::fs::read_to_string(r2.backed_up.as_ref().unwrap()).unwrap();
        assert!(bak_after_first_rerun.contains("aoide.a = 1;"));

        let r3 = emit_generated(&path, &[opt("aoide.c", "c", Some("3"))], || true).unwrap();
        let bak_after_second_rerun = std::fs::read_to_string(r3.backed_up.as_ref().unwrap()).unwrap();
        assert!(bak_after_second_rerun.contains("aoide.b = 2;"), "the .bak must hold the MOST RECENT prior generation, not the first");
        assert!(!bak_after_second_rerun.contains("aoide.a = 1;"));
        assert_eq!(r2.backed_up, r3.backed_up, "the .bak path itself never changes across reruns");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── The generated file against a REAL module eval -- #[ignore]'d: needs
    // `nix`/network, per crates/song/src/widgets.rs's identical precedent. Run
    // by hand via the discipline's own `nix develop -c cargo test` (which has
    // `nix` on PATH); the package's sandboxed checkPhase has neither. Point
    // $AOIDE_SONG_TEMPLATES at a built `lyra-songbook`'s `share/lyra/songbook`
    // (or run from a checkout where the exe-sibling resolves) so
    // `read_aoide_options` finds the shipped set. ──

    /// ONBOARD.md's own P-I3 gate: "generated file evaluates (`nix eval` a
    /// host importing it)" — stronger than `nix-instantiate --parse`
    /// (syntax only): this proves the file MERGES cleanly against the real
    /// `aoide.*` option declarations when a host imports it, catching a
    /// formatter bug `--parse` alone would miss (a duplicate/conflicting
    /// definition, a function-head arg the module system doesn't
    /// recognize). Cheap equivalent of "a host importing it": `evalModules`
    /// over the same walked `modules/` tree [`lib/options.nix`] uses, plus
    /// the generated file, forcing `.config.aoide` this time (not just
    /// `.options`) — never a full host's `system.build.toplevel` (the
    /// brief's own "do not `nix build` the full package" caution extends to
    /// not pulling in hardware/graphics/home-manager weight this check has
    /// no need for).
    ///
    /// The checkout this probe imports is named by CWD-INDEPENDENT compile-time
    /// knowledge (this crate's own manifest dir), never by a runtime walk-up:
    /// `lyra onboard` itself needs no checkout any more, so the only place a
    /// checkout may be named is a TEST that exercises the module system.
    #[test]
    #[ignore = "shells to a real `nix eval`; same sandbox constraint as the eval tests above"]
    fn generated_file_imports_cleanly_into_a_real_module_eval() {
        let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("crates/lyra -> pkgs/aoide -> the checkout")
            .to_path_buf();
        let options = read_aoide_options().expect("the shipped option set must resolve");

        let root = unique_tmp("lyra-onboard-real-import");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        std::fs::write(&path, render_aoide_nix(&options)).unwrap();

        // Nix double-quoted string literals -- `unique_tmp`/the checkout root
        // are plain ASCII paths with no `"`/`\`/`${` to escape.
        let checkout_lit = format!("\"{}\"", checkout.display());
        let path_lit = format!("\"{}\"", path.display());
        let probe = format!(
            r#"
            let
              flake = builtins.getFlake {checkout_lit};
              lib = flake.inputs.nixpkgs.lib;
              pkgs = import flake.inputs.nixpkgs {{ system = "x86_64-linux"; }};
              # The whole tree, as a consumer takes it: the two aggregates. The
              # old `lib/walk.nix` handed every `.nix` under `modules/` to the
              # evaluator; the aggregates are what name that set now, and they
              # are what a whole-tree consumer imports.
              discovered = [
                ({checkout_lit} + "/modules/dendrites")
                ({checkout_lit} + "/modules/nucleus")
              ];
              evaled = lib.evalModules {{
                modules = discovered ++ [ {{ config._module.check = false; }} {path_lit} ];
                specialArgs = {{
                  inputs = flake.inputs;
                  inherit pkgs;
                  username = "khoa";
                  host = "aoide-onboard-import-probe";
                  system = "x86_64-linux";
                }};
              }};
            in
            evaled.config.aoide.enable
            "#
        );
        let probe_file = root.join("probe.nix");
        std::fs::write(&probe_file, probe).unwrap();

        let output = std::process::Command::new("nix")
            .args(["eval", "--json", "--impure", "-f", probe_file.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "importing the generated aoide.nix into a real module eval failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "shells to real `nix-instantiate`; same sandbox constraint as the eval test above"]
    fn generated_file_is_nix_instantiate_parse_clean() {
        let root = unique_tmp("lyra-onboard-parse-clean");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("aoide.nix");
        let text = render_aoide_nix(&[
            opt("aoide.a2a.port", "The A2A HTTP port.", Some("8710")),
            opt("aoide.auditLog", "Path to the single audit log.", Some(r#""/home/${config.aoide.user}/Aoide/log""#)),
            opt("aoide.livery.base16.base00", "base16 base00: default background.", None),
        ]);
        std::fs::write(&path, text).unwrap();

        let output = std::process::Command::new("nix-instantiate").args(["--parse", path.to_str().unwrap()]).output().unwrap();
        assert!(
            output.status.success(),
            "nix-instantiate --parse failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
