//! A stand-in for `aoide`, for this crate's exec tests (feature `fakes`). It
//! prints the fixture schema for `schema --json` and the fixture config for
//! `config --json`, unless `FAKE_AOIDE_NO_SCHEMA` or `FAKE_AOIDE_NO_CONFIG` is
//! set. Each call is appended to the file `FAKE_AOIDE_RECORD` names, as one
//! JSON line: its arguments, its environment and its stdin. Any other call
//! writes `FAKE_AOIDE_STDOUT_BYTES` bytes of `x` to stdout, then the text of
//! `FAKE_AOIDE_STDERR` and `FAKE_AOIDE_STDERR_BYTES` bytes of `x` to stderr,
//! and exits with `FAKE_AOIDE_EXIT` (default 0).

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};

const SCHEMA: &str = include_str!("../../fixtures/schema.json");
const CONFIG: &str = include_str!("../../fixtures/config.json");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut stdin = String::new();
    let _ = io::stdin().read_to_string(&mut stdin);
    record(&args, &stdin);
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["schema", "--json"] if unset("FAKE_AOIDE_NO_SCHEMA") => print!("{SCHEMA}"),
        ["config", "--json"] if unset("FAKE_AOIDE_NO_CONFIG") => print!("{CONFIG}"),
        _ => refuse(),
    }
}

fn unset(name: &str) -> bool {
    std::env::var_os(name).is_none()
}

fn record(args: &[String], stdin: &str) {
    let Some(path) = std::env::var_os("FAKE_AOIDE_RECORD") else {
        return;
    };
    let env: BTreeMap<String, String> = std::env::vars_os()
        .map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()))
        .collect();
    let line = serde_json::json!({"args": args, "env": env, "stdin": stdin});
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("the record file opens");
    writeln!(file, "{line}").expect("the record line is written");
}

fn refuse() -> ! {
    let mut out = io::stdout().lock();
    write_bytes(&mut out, count("FAKE_AOIDE_STDOUT_BYTES"));
    let _ = out.flush();
    let mut err = io::stderr().lock();
    let _ = err.write_all(std::env::var("FAKE_AOIDE_STDERR").unwrap_or_default().as_bytes());
    write_bytes(&mut err, count("FAKE_AOIDE_STDERR_BYTES"));
    std::process::exit(count("FAKE_AOIDE_EXIT") as i32)
}

fn count(name: &str) -> usize {
    std::env::var(name).ok().and_then(|n| n.parse().ok()).unwrap_or(0)
}

fn write_bytes(w: &mut impl Write, mut left: usize) {
    let chunk = [b'x'; 64 * 1024];
    while left > 0 {
        let n = left.min(chunk.len());
        let _ = w.write_all(&chunk[..n]);
        left -= n;
    }
}
