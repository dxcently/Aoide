use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use aoide_desktop_shell::blocking::PATHS;
use aoide_desktop_shell::exec::{Exec, Flag, Schema};
use aoide_desktop_shell::palette::{call, check, palette, CallError, Refusal, MAIN_GRANTS};

const SCHEMA: &str = include_str!("../fixtures/palette-schema.json");
const FAKE: &str = env!("CARGO_BIN_EXE_fake-aoide");

fn schema() -> Schema {
    Schema::parse(SCHEMA).expect("the palette fixture parses")
}

/// The paths of the palette, as words, in the order it offers them.
fn offered(schema: &Schema) -> Vec<String> {
    palette(schema, MAIN_GRANTS).iter().map(|row| row.path.join(" ")).collect()
}

#[test]
fn the_palette_offers_only_granted_commands_the_floor_does_not_hold() {
    assert_eq!(offered(&schema()), ["config", "graph", "session find"]);
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
        let refused = check(&s, MAIN_GRANTS, words, &[]);
        assert!(matches!(refused, Err(Refusal::Floored { .. })), "{words:?}: {refused:?}");
        assert!(!offered(&s).contains(&words.join(" ")), "{words:?} is offered");
    }
}

#[test]
fn the_six_blocking_paths_are_refused_as_blocking_and_are_not_offered() {
    let s = schema();
    assert_eq!(PATHS.len(), 6);
    for words in PATHS {
        let refused = check(&s, MAIN_GRANTS, words, &[]);
        assert!(matches!(refused, Err(Refusal::Blocking(_))), "{words:?}: {refused:?}");
        assert!(!offered(&s).contains(&words.join(" ")), "{words:?} is offered");
    }
}

#[test]
fn a_granted_command_the_floor_holds_is_still_refused() {
    let s = schema();
    let grants: &[&[&str]] = &[&["send"], &["config", "set"], &["graph", "link"]];
    let granted_and_floored: &[&[&str]] = &[&["send"], &["config", "set"], &["graph", "link"]];
    for words in granted_and_floored {
        assert!(
            matches!(check(&s, grants, words, &[]), Err(Refusal::Floored { .. })),
            "{words:?} is granted and floored, so it is refused"
        );
    }
}

#[test]
fn an_ungranted_command_is_refused_as_not_granted() {
    let s = schema();
    let refused = check(&s, MAIN_GRANTS, &["node", "status"], &[]);
    assert_eq!(refused, Err(Refusal::NotGranted("node status".to_owned())));
    assert!(!offered(&s).contains(&"node status".to_owned()));
}

#[test]
fn a_command_the_schema_does_not_declare_is_unknown() {
    let s = schema();
    let refused = check(&s, &[&["nope"]], &["nope"], &[]);
    assert_eq!(refused, Err(Refusal::Unknown("nope".to_owned())));
}

#[test]
fn a_node_or_typed_flag_is_refused_on_any_call() {
    let s = schema();
    for flag in ["node", "typed"] {
        let flags = [Flag { name: flag, value: Some("x") }];
        let refused = check(&s, MAIN_GRANTS, &["graph"], &flags);
        assert!(matches!(refused, Err(Refusal::Floored { .. })), "--{flag}: {refused:?}");
    }
}

#[test]
fn the_palette_spells_every_string_it_offers() {
    let rows = palette(&schema(), MAIN_GRANTS);
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
    let before = records(&record);
    let flags = [Flag { name: "follow", value: None }];
    let err = call(&exec, MAIN_GRANTS, &["session", "trace"], &flags, &["ab"]).expect_err("refused");
    assert!(matches!(err, CallError::Refused(Refusal::Blocking(_))), "{err:?}");
    assert_eq!(records(&record), before, "no child started");
}
