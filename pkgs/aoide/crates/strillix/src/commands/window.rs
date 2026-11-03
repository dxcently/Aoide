//! `strillix window list` and `strillix window watch`: the compositor's windows, read through
//! strillix's CLI door. `list` is one read of the request socket and answers every client in
//! hyprctl's order. `watch` follows the event socket through `hypr::follow_window_events` and writes
//! one line per window event; it runs until the compositor is gone, and ends at its next write once
//! nothing reads its lines. Neither keeps window state: core stores nothing from them, and the audit
//! log records each call that returns. A signal (Ctrl-C, SIGTERM) ends a watch without a return, so
//! it writes no audit line, as for any verb.
//!
//! `watch` differs from the other verbs in two ways. It writes its lines itself, not through
//! `door::say`, because `say_raw` drops the `io::Error` that `stdout-failed` reports. And it ignores
//! SIGPIPE only while it follows, so a reader that has left is a write error and not a signal; the
//! default is back before the door prints (see `handle_watch`).

use crate::daemon::Door;
use crate::dispatch::Invocation;
use crate::hypr::{
    follow_window_events, hypr_event_socket_path, hyprctl_clients, HyprWindowEvent, WatchEnd,
    WatchEvent,
};
use crate::output::{io_cause, Fix, Kind, Outcome};
use crate::registry::{cmd, Registry};
use aoide_protocol::untrusted::clean_line;
use serde_json::{json, Value};
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::time::Duration;

/// How long `watch` waits between a `lost` and the next connect. It is the only deliberate wait:
/// each connect waits at most `CONNECT_BUDGET` (2 s), and a reader that stops reading without
/// closing its end holds the watch at its next write.
const BACKOFF: Duration = Duration::from_secs(2);

const NO_LIST: &str = "no compositor to read";
const NO_WATCH: &str = "no compositor to watch";
const NO_LIST_WHY: &str = "HYPRLAND_INSTANCE_SIGNATURE or XDG_RUNTIME_DIR is unset, or Hyprland's request socket did not answer";
const NO_WATCH_WHY: &str = "HYPRLAND_INSTANCE_SIGNATURE or XDG_RUNTIME_DIR is unset, so there is no event socket to follow";
const NO_COMPOSITOR_FIX: &str = "nothing to run here: the windows are Hyprland's, so this needs a running Hyprland session";

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["window", "list"],
        summary: "The windows the compositor shows, in its own order. Text gives address, class and workspace; --json adds title, pid, mapped and cwd. Nothing is filtered by class. With no compositor it is a refusal, never an empty list.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_list,
        examples: ["window list", "window list --json"],
        brief: "The compositor's windows, in its own order.",
        doors: &[Door::Cli],
    ));
    r.insert(cmd!(
        path: ["window", "watch"],
        summary: "Follow the compositor's window events: a line for each window that appears, changes or closes, with connected and lost around each connection to the event socket. It runs until the compositor is gone, and ends at its next write once nothing reads its lines, with a refusal saying which. With --json every stdout line is one event object.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_watch,
        examples: ["window watch", "window watch --json"],
        brief: "Follow the compositor's window events.",
        doors: &[Door::Cli],
    ));
}

/// One window as `window list` reports it. The field names are the wire: the JSON keys.
struct Window {
    address: String,
    class: String,
    title: String,
    workspace: Option<i64>,
    pid: Option<i32>,
    mapped: bool,
    cwd: String,
}

impl Window {
    fn json(&self) -> Value {
        json!({
            "address": self.address,
            "class": self.class,
            "title": self.title,
            "workspace": self.workspace,
            "pid": self.pid,
            "mapped": self.mapped,
            "cwd": self.cwd,
        })
    }

    /// One text line: address, class and workspace id or `-`. The class goes through `clean_line`
    /// because an app names it, and a bidi override or a line separator in a name would reorder or
    /// split the line. The address is the compositor's own hex, as the compositor gave it, trimmed.
    /// The title is left out: this message is the audit log's, and a title can name a page or a
    /// person.
    fn line(&self) -> String {
        let workspace = self.workspace.map_or_else(|| "-".to_string(), |id| id.to_string());
        format!("  {} {} {}", self.address, clean_line(&self.class), workspace)
    }
}

/// A client of `hyprctl clients -j` as a window, by the rules the roster's terminal reads keep
/// (`hypr::terminals`): no class filter, a client with no address is dropped, a pid must fit i32
/// and be positive, and a client with no `mapped` key is shown.
fn window_from_client(client: &Value) -> Option<Window> {
    let address = client.get("address").and_then(Value::as_str)?.trim().to_string();
    if address.is_empty() {
        return None;
    }
    let pid = client
        .get("pid")
        .and_then(Value::as_i64)
        .and_then(|p| i32::try_from(p).ok())
        .filter(|p| *p > 0);
    Some(Window {
        address,
        class: text_field(client, "class"),
        title: text_field(client, "title"),
        workspace: client.get("workspace").and_then(|w| w.get("id")).and_then(Value::as_i64),
        pid,
        mapped: client.get("mapped").and_then(Value::as_bool).unwrap_or(true),
        cwd: cwd_of(pid),
    })
}

fn text_field(client: &Value, key: &str) -> String {
    client.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// The working directory of `pid` from `/proc/<pid>/cwd`; `""` with no pid or when the read fails.
fn cwd_of(pid: Option<i32>) -> String {
    pid.and_then(|pid| std::fs::read_link(format!("/proc/{pid}/cwd")).ok())
        .and_then(|cwd| cwd.to_str().map(str::to_string))
        .unwrap_or_default()
}

/// The refusal for a verb with no compositor to ask. `data.reason` is `no-compositor`, the key a
/// caller matches.
fn no_compositor(cmd: &str, what: &str, why: &str) -> Outcome {
    Outcome::refuse(cmd, Kind::Refused, what, why, Fix::None(NO_COMPOSITOR_FIX))
        .with_fields(json!({ "reason": "no-compositor" }))
}

fn handle_list(_inv: &Invocation) -> Outcome {
    let cmd = "window.list";
    let Some(clients) = hyprctl_clients() else {
        return no_compositor(cmd, NO_LIST, NO_LIST_WHY);
    };
    let windows: Vec<Window> = clients.iter().filter_map(window_from_client).collect();
    let n = windows.len();
    let mut lines = vec![format!("{n} window{}", if n == 1 { "" } else { "s" })];
    lines.extend(windows.iter().map(Window::line));
    let data: Vec<Value> = windows.iter().map(Window::json).collect();
    Outcome::ok(cmd, lines.join("\n")).with_data(json!({ "windows": data }))
}

fn handle_watch(inv: &Invocation) -> Outcome {
    // main resets SIGPIPE to its default for `preview tree | head`. The watch ignores it only while
    // it follows, so a reader that has left is a write error, and the watch ends with its audited
    // `stdout-closed` refusal. The default is back before the door prints that refusal.
    // SAFETY: `SIG_IGN` installs no handler, so no code of ours runs when a signal arrives.
    let previous = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
    let stdout = std::io::stdout();
    let outcome = follow(
        hypr_event_socket_path().as_deref(),
        inv.flag_present("json"),
        &mut stdout.lock(),
        BACKOFF,
    );
    if previous != libc::SIG_ERR {
        // SAFETY: `previous` is the disposition `signal` returned, so no new handler is installed.
        unsafe { libc::signal(libc::SIGPIPE, previous) };
    }
    outcome
}

/// Writes each event of the follow as one line, and maps how the follow ended to an outcome. The
/// connects and reads are `hypr::follow_window_events`'s.
fn follow(sock: Option<&Path>, json: bool, out: &mut impl Write, backoff: Duration) -> Outcome {
    let cmd = "window.watch";
    let Some(sock) = sock else {
        return no_compositor(cmd, NO_WATCH, NO_WATCH_WHY);
    };
    let end = follow_window_events(sock, backoff, |event| {
        let (kind, address) = match event {
            WatchEvent::Connected => ("connected", None),
            WatchEvent::Lost => ("lost", None),
            WatchEvent::Window(HyprWindowEvent::Appeared { address }) => {
                ("appeared", Some(prefixed(address)))
            }
            WatchEvent::Window(HyprWindowEvent::Changed { address }) => {
                ("changed", Some(prefixed(address)))
            }
            WatchEvent::Window(HyprWindowEvent::Closed { address }) => {
                ("closed", Some(prefixed(address)))
            }
        };
        write_line(out, json, kind, address)
    });
    match end {
        WatchEnd::NoCompositor(e) => {
            let cause = io_cause("connect to", sock, &e);
            no_compositor(cmd, NO_WATCH, &cause.why).with_detail(cause.detail)
        }
        WatchEnd::Unwritten(e) if e.kind() == ErrorKind::BrokenPipe => stdout_closed(cmd, &e),
        WatchEnd::Unwritten(e) => stdout_failed(cmd, &e),
    }
}

/// One line of the stream. With `--json` it is one object: `type` names the event, and `address`
/// is there when the event names a window. Without it, the same words in plain text.
fn write_line(
    out: &mut impl Write,
    json: bool,
    kind: &str,
    address: Option<String>,
) -> std::io::Result<()> {
    let text = match (json, address) {
        (true, Some(address)) => json!({ "type": kind, "address": address }).to_string(),
        (true, None) => json!({ "type": kind }).to_string(),
        (false, Some(address)) => format!("{kind} {address}"),
        (false, None) => kind.to_string(),
    };
    // One write, newline included: a failed write then leaves nothing in stdout's buffer. A
    // `writeln!` leaves its text there when the newline's write fails, and the flush at exit sends
    // that text to the closed pipe, ending the process by signal instead of with the refusal's exit 1.
    out.write_all(format!("{text}\n").as_bytes())?;
    out.flush()
}

/// Hyprland's event socket gives the address bare; `window`'s addresses carry the `0x` hyprctl prints.
fn prefixed(address: &str) -> String {
    if address.starts_with("0x") {
        address.to_string()
    } else {
        format!("0x{address}")
    }
}

/// The reader of the watch's stdout has gone: the write that found it closed is the detail.
fn stdout_closed(cmd: &str, e: &std::io::Error) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Failed,
        "the watch's stdout is closed",
        "nothing reads the lines",
        Fix::None("the reader left, and only a reader that stays on the watch's stdout keeps it running"),
    )
    .with_fields(json!({ "reason": "stdout-closed" }))
    .with_detail(e.to_string())
}

/// A write failed for another reason than a closed reader. The why names the kind in words, and the
/// OS error is the detail.
fn stdout_failed(cmd: &str, e: &std::io::Error) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Failed,
        "the window events are not written",
        format!("the write failed ({})", e.kind()),
        Fix::None("the failure is in the stdout the watch writes to, not in the watch: fix that, then run it again"),
    )
    .with_fields(json!({ "reason": "stdout-failed" }))
    .with_detail(e.to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::bridge::testutil::env_lock;
    use crate::hypr::testutil::{fake_hypr_clients, fill_backlog, unique_stage, EnvVars};
    use crate::output::Status;
    use aoide_test_support::{accept_one, finishes_within, DELIVERY_BUDGET};
    use std::collections::BTreeMap;
    use std::io::{ErrorKind, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Instant;

    fn invocation(path: &[&str]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: vec![],
            flags: BTreeMap::new(),
            door: Door::Cli,
        }
    }

    fn refusal(out: &Outcome) -> &Value {
        &out.data.as_ref().unwrap()["refusal"]
    }

    fn why(out: &Outcome) -> &str {
        refusal(out)["why"].as_str().expect("a refusal names why")
    }

    fn detail(out: &Outcome) -> &str {
        out.data.as_ref().unwrap()["detail"].as_str().expect("the outcome carries its OS detail")
    }

    /// Every client, in hyprctl's order, with no class filter: the empty address is dropped, a pid
    /// past i32 is null, a pid of 0 or -1 is null, a missing `mapped` reads as shown, and a `mapped`
    /// of false is kept. The order is one no sort gives.
    #[test]
    fn list_keeps_every_client_in_hyprctls_order_with_the_wire_fields() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let own = std::process::id();
        let clients = json!([
            { "address": "0xDD", "class": "kitty", "title": "no mapped key", "workspace": { "id": 6 } },
            { "address": "0xBB", "class": "firefox", "title": "Mozilla Firefox", "workspace": { "id": 1 } },
            { "address": "0xEE", "class": "kitty", "title": "no workspace" },
            { "address": "0xAA", "class": "kitty", "title": "shell", "pid": own,
              "workspace": { "id": 3 }, "mapped": true },
            { "address": "", "class": "kitty", "title": "no address", "pid": own, "workspace": { "id": 2 } },
            { "address": "0xCC", "class": "foot", "title": "pid past i32", "pid": 4294968530u64,
              "workspace": { "id": 5 }, "mapped": true },
            { "address": "0x99", "class": "kitty", "title": "unmapped", "workspace": { "id": 2 },
              "mapped": false },
            { "address": "0x66", "class": "foot", "title": "pid zero", "pid": 0, "workspace": { "id": 4 } },
            { "address": "0x55", "class": "zed", "title": "pid minus one", "pid": -1, "workspace": { "id": 7 } },
        ]);
        let (_socket, root) = fake_hypr_clients("window-list", move || clients.clone());
        let cwd = std::env::current_dir().unwrap().to_string_lossy().into_owned();

        let out = handle_list(&invocation(&["window", "list"]));

        assert_eq!(out.status, Status::Ok, "{out:?}");
        assert_eq!(
            out.data.as_ref().unwrap()["windows"],
            json!([
                { "address": "0xDD", "class": "kitty", "title": "no mapped key", "workspace": 6,
                  "pid": null, "mapped": true, "cwd": "" },
                { "address": "0xBB", "class": "firefox", "title": "Mozilla Firefox", "workspace": 1,
                  "pid": null, "mapped": true, "cwd": "" },
                { "address": "0xEE", "class": "kitty", "title": "no workspace", "workspace": null,
                  "pid": null, "mapped": true, "cwd": "" },
                { "address": "0xAA", "class": "kitty", "title": "shell", "workspace": 3,
                  "pid": own, "mapped": true, "cwd": cwd },
                { "address": "0xCC", "class": "foot", "title": "pid past i32", "workspace": 5,
                  "pid": null, "mapped": true, "cwd": "" },
                { "address": "0x99", "class": "kitty", "title": "unmapped", "workspace": 2,
                  "pid": null, "mapped": false, "cwd": "" },
                { "address": "0x66", "class": "foot", "title": "pid zero", "workspace": 4,
                  "pid": null, "mapped": true, "cwd": "" },
                { "address": "0x55", "class": "zed", "title": "pid minus one", "workspace": 7,
                  "pid": null, "mapped": true, "cwd": "" },
            ])
        );
        assert_eq!(
            out.message,
            "8 windows\n  0xDD kitty 6\n  0xBB firefox 1\n  0xEE kitty -\n  0xAA kitty 3\n  0xCC foot 5\n  0x99 kitty 2\n  0x66 foot 4\n  0x55 zed 7"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A class with a bidi override and a line separator in it comes out of the text line without
    /// them: the override would reorder the line, the separator would split it.
    #[test]
    fn the_text_line_strips_the_bidi_and_line_separator_characters_from_a_class() {
        let window = Window {
            address: "0xAA".to_string(),
            class: "kit\u{202e}ty\u{2028}x".to_string(),
            title: String::new(),
            workspace: Some(3),
            pid: None,
            mapped: true,
            cwd: String::new(),
        };

        assert_eq!(window.line(), "  0xAA kittyx 3");
    }

    #[test]
    fn list_without_a_signature_is_the_no_compositor_refusal_never_an_empty_list() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");

        let out = handle_list(&invocation(&["window", "list"]));

        assert_eq!(out.status, Status::Error, "{out:?}");
        assert_eq!(refusal(&out)["kind"], "refused");
        assert_eq!(refusal(&out)["what"], "no compositor to read");
        assert_eq!(out.data.as_ref().unwrap()["reason"], "no-compositor");
        assert!(out.data.as_ref().unwrap().get("detail").is_none(), "{out:?}");
    }

    #[test]
    fn list_with_a_signature_whose_socket_is_missing_is_the_no_compositor_refusal() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = unique_stage("window-list-missing");
        let _env = EnvVars::save(&["XDG_RUNTIME_DIR", "HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "nowhere");

        let out = handle_list(&invocation(&["window", "list"]));

        assert_eq!(out.status, Status::Error, "{out:?}");
        assert_eq!(out.data.as_ref().unwrap()["reason"], "no-compositor");
        assert_eq!(refusal(&out)["what"], "no compositor to read");
        assert!(!out.message.contains("os error"), "{}", out.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// What the first connection to the fake event socket is sent. One title is not UTF-8.
    const EVENTS: &[u8] = b"openwindow>>55aabb,3,kitty,t\n\
        movewindowv2>>55aabb,3,ws\n\
        windowtitlev2>>55aabb,t\xff2\n\
        activewindow>>kitty,t2\n\
        closewindow>>55aabb\n";

    fn event_socket(root: &Path) -> PathBuf {
        let dir = root.join("hypr").join("test");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(".socket2.sock")
    }

    /// A fake event socket that accepts one connection, sends it `script`, and closes it. The accept
    /// is bounded by `accept_one`, so the thread ends or panics within the rig's budget.
    fn serve_script(root: &Path, script: &'static [u8]) -> (PathBuf, std::thread::JoinHandle<()>) {
        let path = event_socket(root);
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let mut conn = accept_one(&listener, "the watch's event connection");
            conn.write_all(script).unwrap();
        });
        (path, server)
    }

    /// A fake event socket that takes two connections and then goes. The first is sent [`EVENTS`]
    /// and closed; the second is closed at once, after the socket file is removed, so the next
    /// connect finds nothing. Both accepts are bounded by `accept_one`.
    fn serve_events(root: &Path) -> (PathBuf, std::thread::JoinHandle<()>) {
        let path = event_socket(root);
        let listener = UnixListener::bind(&path).unwrap();
        let gone = path.clone();
        let server = std::thread::spawn(move || {
            let mut first = accept_one(&listener, "the first event connection");
            first.write_all(EVENTS).unwrap();
            drop(first);
            let second = accept_one(&listener, "the second event connection");
            std::fs::remove_file(&gone).unwrap();
            drop(second);
        });
        (path, server)
    }

    /// Runs `follow` on a thread within the rig's budget, so a follow that never returns fails the
    /// test by name here instead of holding the suite. The writer comes back with the outcome.
    fn follow_within<W: Write + Send + 'static>(
        sock: Option<PathBuf>,
        json: bool,
        out: W,
        backoff: Duration,
    ) -> (Outcome, W) {
        let (_, (outcome, out)) = finishes_within(DELIVERY_BUDGET, move || {
            let mut out = out;
            let outcome = follow(sock.as_deref(), json, &mut out, backoff);
            (outcome, out)
        })
        .expect("the watch did not return within the budget");
        (outcome, out)
    }

    #[test]
    fn watch_writes_a_json_line_per_event_then_ends_when_the_socket_is_gone() {
        let root = unique_stage("window-watch-json");
        let (sock, server) = serve_events(&root);
        let started = Instant::now();

        let (outcome, out) =
            follow_within(Some(sock), true, Vec::<u8>::new(), Duration::from_millis(100));
        server.join().unwrap();

        // Two `lost` lines, each followed by its backoff: the run cannot be quicker than two sleeps.
        assert!(started.elapsed() >= Duration::from_millis(200), "{:?}", started.elapsed());
        assert_eq!(outcome.status, Status::Error, "{outcome:?}");
        assert_eq!(refusal(&outcome)["kind"], "refused");
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "no-compositor");
        let lines: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let kinds: Vec<&str> = lines.iter().map(|l| l["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            ["connected", "appeared", "changed", "changed", "closed", "lost", "connected", "lost"]
        );
        for line in &lines {
            assert!(line.get("status").is_none(), "an event line carries no status: {line}");
            if let Some(address) = line.get("address") {
                assert_eq!(address, "0x55aabb", "{line}");
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn watch_without_json_writes_the_same_events_as_plain_words() {
        let root = unique_stage("window-watch-text");
        let (sock, server) = serve_events(&root);

        let (_, out) = follow_within(Some(sock), false, Vec::<u8>::new(), Duration::from_millis(5));
        server.join().unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "connected\nappeared 0x55aabb\nchanged 0x55aabb\nchanged 0x55aabb\nclosed 0x55aabb\nlost\nconnected\nlost\n"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A socket file with no listener behind it: the connect is refused and nothing is written. The
    /// connect's own OS error is the detail; the refusal's words hold none.
    #[test]
    fn watch_writes_nothing_when_the_socket_file_has_no_listener() {
        let root = unique_stage("window-watch-refused");
        let path = event_socket(&root);
        drop(UnixListener::bind(&path).unwrap());
        // A child another test spawns holds a copy of the listener until it execs, and a connect
        // reaches that copy, so the probe retries. Once refused, the follow finds no listener.
        let deadline = Instant::now() + DELIVERY_BUDGET;
        let os_error = loop {
            match UnixStream::connect(&path) {
                Err(e) => break e.to_string(),
                Ok(_) => {
                    assert!(
                        Instant::now() < deadline,
                        "a connect still reached a listener {:?} after its drop",
                        DELIVERY_BUDGET
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        };

        let (outcome, out) =
            follow_within(Some(path), true, Vec::<u8>::new(), Duration::from_millis(5));

        assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
        assert_eq!(outcome.message, NO_WATCH);
        assert_eq!(refusal(&outcome)["kind"], "refused");
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "no-compositor");
        assert!(detail(&outcome).contains(&os_error), "{}", detail(&outcome));
        assert!(!outcome.message.contains("os error"), "{}", outcome.message);
        assert!(!why(&outcome).contains("os error"), "{}", why(&outcome));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn watch_without_a_socket_path_is_the_no_compositor_refusal_at_once() {
        let (outcome, out) = follow_within(None, true, Vec::<u8>::new(), Duration::from_millis(5));

        assert!(out.is_empty());
        assert_eq!(refusal(&outcome)["what"], "no compositor to watch");
        assert_eq!(outcome.message, "no compositor to watch");
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "no-compositor");
    }

    /// A path past sun_path's 108 bytes cannot be connected. The watch ends at once, with that reason
    /// in its detail, and waits no backoff: a 2 s backoff would show here.
    #[test]
    fn watch_ends_at_once_when_the_socket_path_cannot_be_connected() {
        let root = unique_stage("window-watch-long");
        let long = root.join("h".repeat(120)).join(".socket2.sock");
        let started = Instant::now();

        let (outcome, out) =
            follow_within(Some(long), true, Vec::<u8>::new(), Duration::from_secs(2));

        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
        assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "no-compositor");
        assert!(detail(&outcome).contains("SUN_LEN"), "{}", detail(&outcome));
        assert!(!outcome.message.contains("os error"), "{}", outcome.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An empty socket path names no socket: the kernel refuses the connect, the watch ends at once
    /// with that error as its detail, and nothing is written. The expected detail is the connect's
    /// own error text, a fixture: this does not show the bound, which the backlog test does.
    #[test]
    fn watch_on_an_empty_socket_path_ends_at_once_with_the_connect_error() {
        let expected = UnixStream::connect("").unwrap_err().to_string();
        let started = Instant::now();

        let (outcome, out) =
            follow_within(Some(PathBuf::new()), true, Vec::<u8>::new(), Duration::from_secs(2));

        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
        assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "no-compositor");
        assert!(detail(&outcome).contains(&expected), "{}", detail(&outcome));
    }

    /// A listener that never accepts, its backlog full: the next plain connect blocks in the kernel.
    /// The watch must answer `no-compositor` with the timeout as its detail, not hang on the connect.
    /// It takes its connect budget, so at least 1 s: this is the test that shows the bound.
    #[test]
    fn watch_with_a_full_backlog_is_no_compositor_within_its_connect_budget() {
        let root = unique_stage("window-watch-backlog");
        let path = event_socket(&root);
        let listener = UnixListener::bind(&path).unwrap();

        let _held = fill_backlog(&listener, &path);

        let (tx, rx) = mpsc::channel();
        let sock = path.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let outcome =
                follow(Some(&sock), true, &mut Vec::<u8>::new(), Duration::from_millis(5));
            let _ = tx.send((started.elapsed(), outcome));
        });
        let (elapsed, outcome) = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("the watch was still blocked after 3 s on a full backlog");

        assert!(
            elapsed >= Duration::from_secs(1),
            "the connect gave up after {elapsed:?}, not its budget"
        );
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "no-compositor");
        assert!(detail(&outcome).contains("timed out"), "{}", detail(&outcome));
        drop(listener);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stdout whose reader has gone: every write fails with a broken pipe.
    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A stdout that is full: every write fails with ENOSPC, which is not a closed pipe.
    struct FullDisk;

    impl Write for FullDisk {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from_raw_os_error(28))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A stdout that fails once: the first write whose bytes contain `needle` gets a broken pipe, and
    /// every other write is taken. What got through is kept in `taken`.
    struct FailOnce {
        needle: &'static str,
        failed: bool,
        taken: Vec<u8>,
    }

    impl Write for FailOnce {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if !self.failed && String::from_utf8_lossy(buf).contains(self.needle) {
                self.failed = true;
                return Err(std::io::Error::from(ErrorKind::BrokenPipe));
            }
            self.taken.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A stdout that counts its flushes beside the bytes it takes.
    #[derive(Default)]
    struct Counting {
        taken: Vec<u8>,
        flushes: usize,
    }

    impl Write for Counting {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.taken.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    #[test]
    fn watch_ends_when_nothing_reads_its_lines() {
        let root = unique_stage("window-watch-closed");
        let (path, server) = serve_script(&root, b"");

        let (outcome, _) = follow_within(Some(path), true, ClosedPipe, Duration::from_millis(5));
        server.join().unwrap();

        assert_eq!(outcome.status, Status::Error, "{outcome:?}");
        assert_eq!(refusal(&outcome)["kind"], "failed");
        assert_eq!(refusal(&outcome)["what"], "the watch's stdout is closed");
        assert_eq!(why(&outcome), "nothing reads the lines");
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "stdout-closed");
        assert!(!detail(&outcome).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn watch_ends_with_stdout_failed_when_the_write_fails_for_another_reason() {
        let root = unique_stage("window-watch-full");
        let (path, server) = serve_script(&root, b"");

        let (outcome, _) = follow_within(Some(path), true, FullDisk, Duration::from_millis(5));
        server.join().unwrap();

        assert_eq!(outcome.status, Status::Error, "{outcome:?}");
        assert_eq!(refusal(&outcome)["kind"], "failed");
        assert_eq!(refusal(&outcome)["what"], "the window events are not written");
        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "stdout-failed");
        assert!(detail(&outcome).contains("os error 28"), "{}", detail(&outcome));
        assert!(!outcome.message.contains("os error"), "{}", outcome.message);
        assert!(!why(&outcome).contains("os error"), "{}", why(&outcome));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The write of the first event line fails, once. The watch ends with `stdout-closed` at that
    /// line; one that ignored the failure would reconnect, find nothing listening, and end
    /// `no-compositor` instead.
    #[test]
    fn watch_stops_at_the_first_event_line_whose_write_fails() {
        let root = unique_stage("window-watch-first-line");
        let (path, server) = serve_script(&root, EVENTS);
        let stdout = FailOnce { needle: "appeared", failed: false, taken: Vec::new() };

        let (outcome, stdout) = follow_within(Some(path), true, stdout, Duration::from_millis(5));
        server.join().unwrap();

        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "stdout-closed", "{outcome:?}");
        assert_eq!(String::from_utf8(stdout.taken).unwrap(), "{\"type\":\"connected\"}\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The write of the `lost` line fails, once. The watch ends with `stdout-closed` there, instead
    /// of sleeping its backoff and reconnecting.
    #[test]
    fn watch_stops_when_the_lost_line_cannot_be_written() {
        let root = unique_stage("window-watch-lost");
        let (path, server) = serve_script(&root, b"");
        let stdout = FailOnce { needle: "lost", failed: false, taken: Vec::new() };

        let (outcome, stdout) = follow_within(Some(path), true, stdout, Duration::from_millis(5));
        server.join().unwrap();

        assert_eq!(outcome.data.as_ref().unwrap()["reason"], "stdout-closed", "{outcome:?}");
        assert_eq!(String::from_utf8(stdout.taken).unwrap(), "{\"type\":\"connected\"}\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One flush per line written: a line that sat in a buffer would reach its reader late.
    #[test]
    fn watch_flushes_once_per_line_it_writes() {
        let root = unique_stage("window-watch-flush");
        let (sock, server) = serve_events(&root);

        let (_, counting) =
            follow_within(Some(sock), true, Counting::default(), Duration::from_millis(5));
        server.join().unwrap();

        let lines = counting.taken.iter().filter(|b| **b == b'\n').count();
        assert_eq!(lines, 8);
        assert_eq!(counting.flushes, lines);
        let _ = std::fs::remove_dir_all(&root);
    }
}
