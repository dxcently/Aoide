use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use aoide_desktop_shell::blocking::PATHS;
use aoide_desktop_shell::exec::{Exec, Flag, Schema};
use aoide_desktop_shell::palette::{call, check, main_grants, palette, CallError, Refusal, FLOOR};

const SCHEMA: &str = include_str!("../fixtures/palette-schema.json");
const FAKE: &str = env!("CARGO_BIN_EXE_fake-aoide");

fn schema() -> Schema {
    Schema::parse(SCHEMA).expect("the palette fixture parses")
}

/// A frame's grant set: narrower than the main window's, as V21 section 4.3 has it.
fn frame() -> Vec<Vec<&'static str>> {
    vec![vec!["config"], vec!["graph"], vec!["session", "find"]]
}

/// The paths of the main window's palette, as words, in the order it offers them.
fn offered(schema: &Schema) -> Vec<String> {
    palette(schema, &main_grants(schema)).iter().map(|row| row.path.join(" ")).collect()
}

#[test]
fn the_main_palette_offers_every_command_the_floor_does_not_hold() {
    assert_eq!(
        offered(&schema()),
        ["config", "graph", "melete status", "node status", "session find"]
    );
}

#[test]
fn a_read_such_as_graph_is_offered_and_its_floored_neighbour_is_not() {
    let s = schema();
    assert!(offered(&s).contains(&"graph".to_owned()));
    assert!(!offered(&s).contains(&"graph link".to_owned()));
}

#[test]
fn every_named_floor_class_and_every_marked_command_is_refused_and_absent_from_the_palette() {
    let s = schema();
    let grants = main_grants(&s);
    let floored: &[&[&str]] = &[
        &["send"],
        &["spawn"],
        &["session", "hook"],
        &["content", "approve"],
        &["graph", "link"],
        &["secrets", "put"],
        &["pair", "ask"],
        &["mesh", "join"],
        &["node", "allow"],
        &["node", "add"],
        &["identity"],
        &["session", "grant"],
        &["config", "set"],
        &["hooks", "install"],
        &["update"],
        &["daemon"],
        &["mneme", "call"],
        &["melete", "call"],
    ];
    for words in floored {
        let refused = check(&s, &grants, words, &[]);
        assert!(matches!(refused, Err(Refusal::Floored { .. })), "{words:?}: {refused:?}");
        assert!(!offered(&s).contains(&words.join(" ")), "{words:?} is offered");
    }
}

#[test]
fn the_six_blocking_paths_are_refused_as_blocking_and_are_not_offered() {
    let s = schema();
    let grants = main_grants(&s);
    assert_eq!(PATHS.len(), 6);
    for words in PATHS {
        let refused = check(&s, &grants, words, &[]);
        assert!(matches!(refused, Err(Refusal::Blocking(_))), "{words:?}: {refused:?}");
        assert!(!offered(&s).contains(&words.join(" ")), "{words:?} is offered");
    }
}

#[test]
fn a_granted_command_the_floor_holds_is_still_refused() {
    let s = schema();
    let grants = vec![vec!["send"], vec!["config", "set"], vec!["graph", "link"]];
    for words in [&["send"][..], &["config", "set"], &["graph", "link"]] {
        assert!(
            matches!(check(&s, &grants, words, &[]), Err(Refusal::Floored { .. })),
            "{words:?} is granted and floored, so it is refused"
        );
    }
}

#[test]
fn an_ungranted_command_is_refused_as_not_granted() {
    let s = schema();
    let refused = check(&s, &frame(), &["node", "status"], &[]);
    assert_eq!(refused, Err(Refusal::NotGranted("node status".to_owned())));
    let offered_to_frame: Vec<String> =
        palette(&s, &frame()).iter().map(|row| row.path.join(" ")).collect();
    assert!(!offered_to_frame.contains(&"node status".to_owned()));
}

#[test]
fn a_command_the_schema_does_not_declare_is_unknown() {
    let s = schema();
    let refused = check(&s, &[vec!["nope"]], &["nope"], &[]);
    assert_eq!(refused, Err(Refusal::Unknown("nope".to_owned())));
}

#[test]
fn a_node_or_typed_flag_is_refused_on_any_call() {
    let s = schema();
    let grants = main_grants(&s);
    for flag in ["node", "typed"] {
        let flags = [Flag { name: flag, value: Some("x") }];
        let refused = check(&s, &grants, &["graph"], &flags);
        assert!(matches!(refused, Err(Refusal::Floored { .. })), "--{flag}: {refused:?}");
    }
}

#[test]
fn the_palette_spells_every_string_it_offers() {
    let s = schema();
    let rows = palette(&s, &main_grants(&s));
    let config = rows.iter().find(|row| row.path == ["config"]).expect("config is offered");
    assert_eq!(config.brief, "Show the config.\\u{202e}");
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aoide-desk-palette-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    dir
}

fn records(record: &Path) -> usize {
    fs::read_to_string(record).unwrap_or_default().lines().count()
}

#[test]
fn a_blocking_path_through_call_is_refused_and_starts_no_child() {
    let dir = scratch("blocking-call");
    let record = dir.join("record.jsonl");
    let env = vec![
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (OsString::from("FAKE_AOIDE_RECORD"), record.clone().into_os_string()),
    ];
    let exec = Exec::load(Path::new(FAKE), &dir.join("home"), env).expect("the fake loads");
    let grants = main_grants(exec.schema());
    let before = records(&record);
    let flags = [Flag { name: "follow", value: None }];
    let err = call(&exec, &grants, &["session", "trace"], &flags, &["ab"]).expect_err("refused");
    assert!(matches!(err, CallError::Refused(Refusal::Blocking(_))), "{err:?}");
    assert_eq!(records(&record), before, "no child started");
}

#[test]
fn a_frame_granted_a_parent_cannot_reach_a_floored_child_through_positionals() {
    let dir = scratch("frame-parent");
    let record = dir.join("record.jsonl");
    let env = vec![
        (OsString::from("PATH"), OsString::from("/usr/bin:/bin")),
        (OsString::from("FAKE_AOIDE_RECORD"), record.clone().into_os_string()),
    ];
    let exec = Exec::load(Path::new(FAKE), &dir.join("home"), env).expect("the fake loads");
    let frame: Vec<Vec<&str>> = vec![vec!["config"]];
    assert!(check(exec.schema(), &frame, &["config"], &[]).is_ok(), "the frame may run config itself");
    let before = records(&record);
    let err = call(&exec, &frame, &["config"], &[], &["set", "key", "value"]).expect_err("refused");
    assert!(
        matches!(err, CallError::Exec(aoide_desktop_shell::exec::ExecError::Resolves { .. })),
        "{err:?}"
    );
    assert_eq!(records(&record), before, "no child started");
}

/// One command the floor must refuse, for each `FLOOR` class: its class and the
/// path a test runs. A new `FLOOR` entry without a row here fails
/// `every_floor_class_has_a_test_case`.
const FLOOR_CASES: &[(&str, &[&str])] = &[
    ("custody", &["secrets", "put"]),
    ("custody", &["secret", "put"]),
    ("trust", &["pair", "ask"]),
    ("trust", &["mesh", "join"]),
    ("trust", &["node", "allow"]),
    ("trust", &["node", "add"]),
    ("trust", &["node", "remove"]),
    ("trust", &["node", "address"]),
    ("trust", &["node", "hub"]),
    ("trust", &["identity"]),
    ("trust", &["session", "grant"]),
    ("trust", &["extension", "grant"]),
    ("self-change", &["config", "set"]),
    ("self-change", &["hooks", "install"]),
    ("self-change", &["update"]),
    ("self-change", &["daemon"]),
    ("self-change", &["node", "advertise"]),
    ("self-change", &["extension", "add"]),
    ("self-change", &["extension", "remove"]),
    ("melete tools", &["melete", "call"]),
    ("mneme writes", &["mneme", "call"]),
];

#[test]
fn every_floor_class_has_a_test_case() {
    for (class, prefix) in FLOOR {
        assert!(
            FLOOR_CASES.iter().any(|(c, path)| c == class && path.starts_with(prefix)),
            "the floor class `{class}` at {prefix:?} has no case in FLOOR_CASES"
        );
    }
}

#[test]
fn every_floor_case_is_refused_by_its_class() {
    let commands: Vec<serde_json::Value> = FLOOR_CASES
        .iter()
        .map(|(_, path)| {
            serde_json::json!({"path": path, "summary": "x", "args": [], "flags": [], "implemented": true})
        })
        .collect();
    let text = serde_json::json!({"commands": commands}).to_string();
    let s = Schema::parse(&text).expect("the floor schema parses");
    let grants = main_grants(&s);
    for (class, path) in FLOOR_CASES {
        let refused = check(&s, &grants, path, &[]);
        assert_eq!(
            refused,
            Err(Refusal::Floored { path: path.join(" "), why: class }),
            "{path:?} is refused by its class"
        );
    }
}
