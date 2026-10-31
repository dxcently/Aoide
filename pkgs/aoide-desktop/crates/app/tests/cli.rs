//! The argument refusal of a build without the smoke feature. A build with the
//! feature takes `--smoke`, and this file is empty there.

#![cfg(not(feature = "smoke"))]

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;

use aoide_desktop_shell::exec::test_doors;

#[test]
fn without_the_smoke_feature_an_argument_is_refused_by_name() {
    let args = [OsString::from("--smoke"), OsString::from("/nonexistent")];
    let app = Path::new(env!("CARGO_BIN_EXE_aoide-desktop"));
    let child = test_doors::start(app, &args, &[], Stdio::piped(), Stdio::piped()).expect("the app starts");
    let out = child.wait_with_output().expect("the app ends");
    assert_eq!(out.status.code(), Some(2), "an argument exits 2");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--smoke"), "the refusal names --smoke: {err}");
}
