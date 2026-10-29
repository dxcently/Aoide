//! A stand-in for `aoide`, for this crate's exec tests (feature `fakes`). It
//! prints the fixture schema for `schema --json` and the fixture config for
//! `config --json`, unless `FAKE_AOIDE_NO_SCHEMA` or `FAKE_AOIDE_NO_CONFIG` is
//! set; `FAKE_AOIDE_CONFIG` replaces the config text. Each call is appended to
//! the file `FAKE_AOIDE_RECORD` names, as one JSON line: its arguments, its
//! environment and its stdin. Any other call sleeps `FAKE_AOIDE_SLEEP` seconds,
//! then writes `FAKE_AOIDE_STDOUT_BYTES` bytes of `x` to stdout, then the text of
//! `FAKE_AOIDE_STDERR` and `FAKE_AOIDE_STDERR_BYTES` bytes of `x` to stderr,
//! and exits with `FAKE_AOIDE_EXIT` (default 0).
//!
//! A held follow is `session trace --follow` or `session watch`, on the id it
//! names: an id starting `hook-` keeps no trace and no PTY, so both refuse; an
//! id starting `pty-` keeps no trace, so only `session watch` answers. A
//! watch's first line is a JSON frame. Both then print each line of the file
//! `FAKE_AOIDE_FOLLOW_FILE` names, one every 30 ms, and wait. SIGINT ends them:
//! they print `{"ok":true,"followed":true}` and exit zero.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::time::Duration;

use aoide_desktop_exec::test_doors;

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
        ["config", "--json"] if unset("FAKE_AOIDE_NO_CONFIG") => print!("{}", config_text()),
        ["session", "trace", ..] if words.contains(&"--follow") => follow(words[words.len() - 1], false),
        ["session", "watch", ..] => follow(words[words.len() - 1], true),
        _ => refuse(),
    }
}

/// Serves one held follow on `id`, or refuses it when `id` has no such stream.
fn follow(id: &str, watch: bool) -> ! {
    let trace_refused = id.starts_with("pty-") || id.starts_with("hook-");
    if (!watch && trace_refused) || (watch && id.starts_with("hook-")) {
        eprintln!("{} refused: the session `{id}` keeps no stream of this kind", if watch { "watch" } else { "trace" });
        std::process::exit(1);
    }
    test_doors::block_sigint().expect("SIGINT is blocked before any thread starts");
    std::thread::spawn(|| {
        test_doors::wait_sigint().expect("SIGINT is taken");
        println!("{{\"ok\":true,\"followed\":true}}");
        std::process::exit(0)
    });
    let mut out = io::stdout();
    if watch {
        let _ = writeln!(out, "{{\"frame\":\"live\",\"id\":\"{id}\"}}");
    }
    let lines = std::env::var("FAKE_AOIDE_FOLLOW_FILE").ok().and_then(|path| std::fs::read(path).ok());
    for line in lines.unwrap_or_default().split(|b| *b == b'\n') {
        let _ = out.write_all(line);
        let _ = out.write_all(b"\n");
        let _ = out.flush();
        std::thread::sleep(Duration::from_millis(30));
    }
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn unset(name: &str) -> bool {
    std::env::var_os(name).is_none()
}

fn config_text() -> String {
    std::env::var("FAKE_AOIDE_CONFIG").unwrap_or_else(|_| CONFIG.to_owned())
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
    std::thread::sleep(std::time::Duration::from_secs(count("FAKE_AOIDE_SLEEP") as u64));
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
