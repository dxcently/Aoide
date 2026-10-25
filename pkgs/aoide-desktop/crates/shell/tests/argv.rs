#![deny(unsafe_code)]
use std::ffi::OsString;
use std::path::Path;

use aoide_desktop_shell::exec::{argv, Arg, ExecError, Flag, Schema};

const AOIDE: &str = "/opt/aoide/bin/aoide";
const FIXTURE: &str = include_str!("../fixtures/schema.json");

fn schema() -> Schema {
    Schema::parse(FIXTURE).expect("the fixture schema parses")
}

fn build(words: &[&str], flags: &[Flag<'_>], positionals: &[&str]) -> Result<Vec<String>, ExecError> {
    let out: Vec<OsString> = argv(Path::new(AOIDE), &schema(), words, flags, positionals)?;
    Ok(out.into_iter().map(|a| a.into_string().expect("argv is UTF-8")).collect())
}

#[test]
fn a_call_is_the_path_then_json_then_flags_then_positionals() {
    let flags = [Flag { name: "id", value: Some("s1") }, Flag { name: "yes", value: None }];
    let got = build(&["send"], &flags, &["hi"]).expect("the call builds");
    assert_eq!(got, [AOIDE, "send", "--json", "--id=s1", "--yes", "--", "hi"]);
}

#[test]
fn a_call_without_positionals_has_no_separator() {
    let got = build(&["config"], &[], &[]).expect("the call builds");
    assert_eq!(got, [AOIDE, "config", "--json"]);
}

#[test]
fn a_valued_flag_of_any_type_is_written_with_equals() {
    let flags = [Flag { name: "limit", value: Some("5") }];
    let got = build(&["melete", "status"], &flags, &[]).expect("the call builds");
    assert_eq!(got, [AOIDE, "melete", "status", "--json", "--limit=5"]);
}

#[test]
fn text_that_starts_with_dashes_or_holds_equals_or_newlines_stays_positional() {
    let texts = ["--yes", "--id=other", "a=b", "line one\nline two"];
    let got = build(&["send"], &[], &texts).expect("the call builds");
    assert_eq!(
        got,
        [AOIDE, "send", "--json", "--", "--yes", "--id=other", "a=b", "line one\nline two"]
    );
}

#[test]
fn a_flag_is_declared_per_path() {
    let flags = [Flag { name: "id", value: Some("s1") }];
    let err = build(&["melete", "status"], &flags, &[]).expect_err("an undeclared flag is refused");
    assert!(
        matches!(&err, ExecError::Undeclared { command, flag } if command == "melete status" && flag == "id"),
        "{err:?}"
    );
}

#[test]
fn a_bare_bool_flag_takes_no_value() {
    let flags = [Flag { name: "yes", value: Some("1") }];
    let err = build(&["send"], &flags, &["hi"]).expect_err("a bool with a value is refused");
    assert!(matches!(err, ExecError::BareFlagTakesNoValue { .. }), "{err:?}");
}

#[test]
fn a_valued_flag_needs_a_value() {
    let flags = [Flag { name: "id", value: None }];
    let err = build(&["send"], &flags, &["hi"]).expect_err("a valued flag without one is refused");
    assert!(matches!(err, ExecError::FlagNeedsValue { .. }), "{err:?}");
}

#[test]
fn an_unknown_path_is_refused() {
    let err = build(&["nope"], &[], &[]).expect_err("an unknown path is refused");
    assert!(matches!(&err, ExecError::UnknownCommand(w) if w == "nope"), "{err:?}");
}

#[test]
fn a_missing_required_positional_is_refused() {
    let err = build(&["send"], &[], &[]).expect_err("a call without its text is refused");
    assert!(
        matches!(&err, ExecError::MissingArgument { command, name } if command == "send" && name == "text"),
        "{err:?}"
    );
}

#[test]
fn a_nul_byte_is_refused_in_a_word_a_flag_value_or_a_positional() {
    let results = [
        build(&["sen\0d"], &[], &[]),
        build(&["send"], &[Flag { name: "id", value: Some("x\0y") }], &["hi"]),
        build(&["send"], &[], &["a\0b"]),
    ];
    for result in &results {
        assert!(matches!(result, Err(ExecError::NulByte { .. })), "{result:?}");
    }
}

#[test]
fn the_schema_keeps_each_paths_flags_and_args() {
    let s = schema();
    let send = s.command(&["send"]).expect("send is declared");
    assert_eq!(send.flags.get("id").map(String::as_str), Some("string"));
    assert_eq!(send.flags.get("yes").map(String::as_str), Some("bool"));
    assert_eq!(send.args, [Arg { name: "text".to_owned(), required: true }]);
    let status = s.command(&["melete", "status"]).expect("melete status is declared");
    assert_eq!(status.flags.get("limit").map(String::as_str), Some("int"));
    assert!(s.command(&["melete"]).is_none(), "a group is not a command");
    assert!(s.command(&["melete status"]).is_none(), "a word with a space is not a path");
}

#[test]
fn a_schema_that_does_not_parse_is_refused() {
    let err = Schema::parse("{").expect_err("a broken schema is refused");
    assert!(matches!(err, ExecError::Schema(_)), "{err:?}");
}
