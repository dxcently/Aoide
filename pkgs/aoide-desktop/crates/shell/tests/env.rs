use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};

use aoide_desktop_shell::exec::{child_env, parse_vault_keys, Child, ExecError};

const CONFIG: &str = include_str!("../fixtures/config.json");
const VAULT: &str = "AOIDE_TEST_VAULT_TOKEN";
const UNKNOWN: &str = "the config could not be read";

fn base() -> Vec<(OsString, OsString)> {
    [
        ("PATH", "/usr/bin"),
        ("HOME", "/home/user"),
        ("AOIDE_SESSION_ID", "sess-1"),
        ("AOIDE_MELETE_KEY", "melete-key"),
        ("AOIDE_MELETE_DB", "/db"),
        (VAULT, "vault-token"),
        ("AOIDE_ROOT", "/root"),
    ]
    .iter()
    .map(|(k, v)| (OsString::from(k), OsString::from(v)))
    .collect()
}

fn names(env: &[(OsString, OsString)]) -> Vec<String> {
    env.iter()
        .map(|(k, _)| k.to_str().expect("a name is UTF-8").to_owned())
        .collect()
}

fn keys() -> Vec<String> {
    vec![VAULT.to_owned()]
}

#[test]
fn an_eidolon_child_loses_session_melete_and_vault_keys() {
    let env = child_env(Child::Eidolon, "", &base(), Ok(keys().as_slice())).expect("the env builds");
    assert_eq!(names(&env), ["PATH", "HOME", "AOIDE_ROOT"]);
}

#[test]
fn an_aoide_child_loses_melete_and_vault_keys_but_keeps_its_session() {
    let env = child_env(Child::Aoide, "send", &base(), Ok(keys().as_slice())).expect("the env builds");
    assert_eq!(names(&env), ["PATH", "HOME", "AOIDE_SESSION_ID", "AOIDE_ROOT"]);
}

#[test]
fn a_melete_command_keeps_the_melete_variables_and_still_loses_vault_keys() {
    let env = child_env(Child::Aoide, "melete", &base(), Ok(keys().as_slice())).expect("the env builds");
    assert_eq!(
        names(&env),
        ["PATH", "HOME", "AOIDE_SESSION_ID", "AOIDE_MELETE_KEY", "AOIDE_MELETE_DB", "AOIDE_ROOT"]
    );
}

#[test]
fn an_eidolon_launch_is_refused_while_the_vault_names_are_unknown() {
    let err = child_env(Child::Eidolon, "", &base(), Err(UNKNOWN)).expect_err("the launch is refused");
    assert!(matches!(err, ExecError::VaultKeysUnknown(_)), "{err:?}");
}

#[test]
fn while_the_vault_names_are_unknown_only_schema_and_config_run() {
    let env = child_env(Child::Aoide, "config", &base(), Err(UNKNOWN)).expect("config still runs");
    let kept = names(&env);
    assert!(kept.iter().all(|n| !n.starts_with("AOIDE_MELETE_")));
    assert!(kept.contains(&VAULT.to_owned()), "an unknown name cannot be removed");
    let err = child_env(Child::Aoide, "send", &base(), Err(UNKNOWN)).expect_err("a verb is refused");
    assert!(matches!(err, ExecError::VaultKeysUnknown(_)), "{err:?}");
}

#[test]
fn the_vault_names_are_the_token_envs_in_the_config() {
    assert_eq!(parse_vault_keys(CONFIG).expect("the config parses"), [VAULT]);
}

#[test]
fn every_vault_in_the_config_names_its_key() {
    let text = r#"{"data":{"config":{"context":{"vaults":{
        "a":{"endpoint":"e","vault":"a","tokenEnv":"KEY_A"},
        "b":{"endpoint":"e","vault":"b","tokenEnv":"KEY_B"}}}}}}"#;
    assert_eq!(parse_vault_keys(text).expect("the config parses"), ["KEY_A", "KEY_B"]);
}

#[test]
fn a_config_without_vaults_names_no_keys() {
    assert_eq!(parse_vault_keys(r#"{"data":{"config":{}}}"#).expect("parses"), Vec::<String>::new());
}

#[test]
fn a_config_that_does_not_parse_leaves_the_names_unknown() {
    let err = parse_vault_keys("{").expect_err("a broken config is refused");
    assert!(matches!(err, ExecError::Config(_)), "{err:?}");
    let err = parse_vault_keys(r#"{"status":"ok"}"#).expect_err("a config without data is refused");
    assert!(matches!(err, ExecError::Config(_)), "{err:?}");
}

#[test]
fn a_non_utf8_name_under_the_melete_prefix_is_scrubbed_unless_the_command_is_melete() {
    let mut base = base();
    base.push((OsString::from_vec(b"AOIDE_MELETE_\xff".to_vec()), OsString::from("x")));
    let env = child_env(Child::Aoide, "send", &base, Ok(keys().as_slice())).expect("the env builds");
    assert!(env.iter().all(|(name, _)| !name.as_bytes().starts_with(b"AOIDE_MELETE_")), "{env:?}");
    let melete = child_env(Child::Aoide, "melete", &base, Ok(keys().as_slice())).expect("the env builds");
    assert!(melete.iter().any(|(name, _)| name.as_bytes() == b"AOIDE_MELETE_\xff"), "{melete:?}");
}
