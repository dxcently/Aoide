//! The help screens through the REAL binary: streams, exit codes, width and
//! color. Every child runs under an isolated AOIDE_ROOT, so the real
//! `~/.aoide` is never read or written.

use std::path::PathBuf;
use std::process::Command;

struct Out {
    stdout: String,
    stderr: String,
    code: i32,
}

fn aoide(tag: &str, env: &[(&str, &str)], args: &[&str]) -> Out {
    let root: PathBuf = std::env::temp_dir().join(format!("aoide-help-screen-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aoide"));
    cmd.args(args)
        .env_remove("NO_COLOR")
        .env_remove("COLUMNS")
        .env("AOIDE_ROOT", &root)
        .env("AOIDE_STAGE_DIR", root.join("stage"))
        .env("AOIDE_STATE_DIR", root.join("state"))
        .env("AOIDE_AUDIT_LOG", root.join("log"))
        .env("PATH", &root);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("the aoide binary runs");
    let _ = std::fs::remove_dir_all(&root);
    Out {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn bare_aoide_prints_the_overview_on_stdout_and_exits_zero() {
    let o = aoide("bare", &[("COLUMNS", "105")], &[]);
    assert_eq!(o.code, 0);
    assert!(o.stderr.is_empty(), "{}", o.stderr);
    assert!(o.stdout.starts_with("aoide — "), "{}", o.stdout);
    assert!(!o.stdout.contains("[usage]"), "{}", o.stdout);
    assert!(!o.stdout.contains('\x1b'), "a pipe is plain: {:?}", o.stdout);
    for (flag, name) in [("--help", "dash"), ("help", "word")] {
        assert_eq!(aoide(name, &[("COLUMNS", "105")], &[flag]).stdout, o.stdout);
    }
}

#[test]
fn a_group_lists_on_stdout_at_exit_zero_and_a_typo_is_a_usage_error_on_stderr() {
    let group = aoide("group", &[("COLUMNS", "105")], &["secrets"]);
    assert_eq!(group.code, 0);
    assert!(group.stdout.contains("commands:\n  serve"), "{}", group.stdout);
    assert_eq!(aoide("group2", &[("COLUMNS", "105")], &["help", "secrets"]).stdout, group.stdout);
    assert_eq!(aoide("group3", &[("COLUMNS", "105")], &["secrets", "--help"]).stdout, group.stdout);

    let typo = aoide("typo", &[("COLUMNS", "105")], &["nodee"]);
    assert_eq!(typo.code, 2);
    assert!(typo.stdout.is_empty(), "{}", typo.stdout);
    assert!(typo.stderr.starts_with("[usage] nodee: unknown command: `nodee`"), "{}", typo.stderr);
}

#[test]
fn the_width_comes_from_columns_and_no_line_outruns_it() {
    for width in [60usize, 80, 120] {
        let w = width.to_string();
        for args in [&[][..], &["secrets"][..], &["help", "send"][..]] {
            let o = aoide("width", &[("COLUMNS", &w)], args);
            for line in o.stdout.lines() {
                let longest = line.split(' ').map(|x| x.chars().count()).max().unwrap_or(0);
                assert!(line.chars().count() <= width.max(longest + 4), "{args:?} at {width}: `{line}`");
            }
        }
    }
}

#[test]
fn color_flag_forces_codes_on_and_off_and_json_and_no_color_win() {
    let on = aoide("on", &[("COLUMNS", "105")], &["--color=always"]);
    assert!(on.stdout.contains("\x1b[1m") && on.stdout.contains("\x1b[36m"), "{:?}", on.stdout);
    assert!(on.stdout.contains("\x1b[2m"), "a stub section is dim: {:?}", on.stdout);

    let off = aoide("off", &[("COLUMNS", "105")], &["--color=never"]);
    assert!(!off.stdout.contains('\x1b'));

    let spaced = aoide("spaced", &[("COLUMNS", "105")], &["--color", "always", "secrets"]);
    assert!(spaced.stdout.contains("\x1b["), "{:?}", spaced.stdout);

    let json = aoide("json", &[], &["--color=always", "--json"]);
    assert!(!json.stdout.contains('\x1b'), "json is data: {:?}", json.stdout);

    let refused = aoide("refused", &[("COLUMNS", "105")], &["--color=always", "nodee"]);
    assert!(refused.stderr.contains("\x1b[33m[usage]\x1b[0m"), "{:?}", refused.stderr);

    let auto = aoide("auto", &[("NO_COLOR", "1")], &["--color=auto"]);
    assert!(!auto.stdout.contains('\x1b'));
}

#[test]
fn a_bad_color_mode_is_a_taught_usage_error() {
    let o = aoide("badcolor", &[], &["--color=loud"]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("`--color loud` is not a color mode") && o.stderr.contains("auto, always, never"), "{}", o.stderr);
}

#[test]
fn the_color_flag_is_not_taken_from_a_wrapped_commands_arguments() {
    let o = aoide("wrapped", &[], &["help", "--", "--color=bogus"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
}
