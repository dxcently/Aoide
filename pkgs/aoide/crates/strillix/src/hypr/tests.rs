//! The desktop's Hyprland tests: the request socket, the focus verbs, the event listener, the
//! terminal roster, and the session-window sweep, each against a fake request socket or a
//! fake `hyprctl` shim, never a live compositor.

use super::client::*;
use super::door::Latch;
use super::focus::*;
use super::listener::*;
use super::sweep::*;
use super::terminals::*;
use super::testutil::*;
use aoide_conduct::graph::{
    load_stage, normalize_addr, sessions_path, write_stage, SessionRecord, SessionsFile,
};
use aoide_test_support::fake_hyprctl;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The recording core stub with aoided up: the door's probe finds the daemon listening, so each
/// call reaches the stub. A test that wants aoided down sets `AOIDE_DAEMON_SOCKET` after this.
fn stub_core_door(tag: &str) -> ((aoide_test_support::EnvSaver, EnvVars), PathBuf) {
    let (core, root) = crate::bridge::testutil::stub_core_door(tag);
    ((core, core_up()), root)
}

/// The sweep as the desktop runs it. Each bind it decides goes through the core door, which the
/// recording stub on `AOIDE_CORE_BIN` answers. The stub writes nothing, so a test that needs core's
/// write records it itself.
fn resolve_pending_session_windows() -> bool {
    resolve_windows(&mut Latch::default())
}

/// The argv of each call the stub core recorded, in order.
fn core_calls(root: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(root.join("calls"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix("argv: ").map(str::to_string))
        .collect()
}

/// The binding core writes once it has accepted a bind: the window and the workspace it observed.
fn record_binding(id: &str, window: &str, workspace: Option<i64>) {
    let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
    let rec = file.sessions.iter_mut().find(|s| s.session_id == id).unwrap();
    rec.window_address = window.into();
    rec.workspace = workspace;
    write_stage(&sessions_path(), &file).unwrap();
}

/// No compositor for the length of a test, whatever the box runs under.
fn no_compositor() -> EnvVars {
    let env = EnvVars::save(&["HYPRLAND_INSTANCE_SIGNATURE"]);
    std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
    env
}

fn match_window_for_ancestry(ancestry: &[i32], clients: &[Value]) -> Option<String> {
    match_window_and_pid(ancestry, clients).map(|(addr, _)| addr)
}

    #[test]
    fn read_timeout_predicate_distinguishes_tick_from_drop() {
        use std::io::{Error, ErrorKind};
        // A read-timeout expiry surfaces as WouldBlock on Unix — that's a tick,
        // not a failure: keep the connection.
        assert!(is_read_timeout(&Error::new(ErrorKind::WouldBlock, "timed out")));
        // Windows/std also documents TimedOut for the same event: accept it too.
        assert!(is_read_timeout(&Error::new(ErrorKind::TimedOut, "timed out")));
        // A genuine socket drop is NOT a timeout → the listener must reconnect.
        assert!(!is_read_timeout(&Error::new(
            ErrorKind::ConnectionReset,
            "node reset"
        )));
        assert!(!is_read_timeout(&Error::new(
            ErrorKind::BrokenPipe,
            "broken pipe"
        )));
        assert!(!is_read_timeout(&Error::new(
            ErrorKind::UnexpectedEof,
            "eof"
        )));
    }

    #[test]
    fn term_window_from_client_drops_a_pid_outside_i32_range_instead_of_wrapping() {
        // Before the fix, `as i32` on an out-of-range i64 would silently wrap
        // into an arbitrary (possibly small, positive) i32 — which then feeds
        // `/proc/<pid>/cwd` right below, and later a widening `as u32` in
        // `reconcile_untracked_terminals`. The checked conversion must drop
        // it instead of wrapping.
        let c = serde_json::json!({
            "address": "0xAABB",
            "class": "kitty",
            "title": "t",
            "pid": (i32::MAX as i64) + 1,
            "mapped": true,
        });
        let w = term_window_from_client(&c).expect("a valid address still yields a TermWindow");
        assert_eq!(w.pid, None, "an out-of-i32-range pid must be dropped, never wrapped");
    }

    #[test]
    fn focus_address_matching_is_prefix_and_case_tolerant() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        // hyprctl reports `0x…` lowercase; the stored windowAddress may differ
        // on case and on a present/absent `0x` prefix — all must match.
        let clients = vec![
            json!({ "address": "0x55aabbccdd00", "class": "kitty" }),
            json!({ "address": "0x1234ef", "class": "foot" }),
        ];
        assert!(window_present(&clients, "0x55aabbccdd00")); // exact
        assert!(window_present(&clients, "55aabbccdd00")); // missing 0x prefix
        assert!(window_present(&clients, "0x55AABBCCDD00")); // upper case
        assert!(window_present(&clients, "55AABBCCDD00")); // both
        assert!(window_present(&clients, "0X1234EF")); // 0X + upper
                                                       // A vanished window is absent.
        assert!(!window_present(&clients, "0xdeadbeef"));
        assert!(!window_present(&clients, ""));
        // Client entry without an address field is ignored, not a false match.
        let noaddr = vec![json!({ "class": "kitty" })];
        assert!(!window_present(&noaddr, "0x1"));

        // Normalisation is idempotent and prefix-agnostic.
        assert_eq!(normalize_addr("0xABC"), "abc");
        assert_eq!(normalize_addr("abc"), "abc");
        assert_eq!(normalize_addr("  0Xabc  "), "abc");
    }

    #[test]
    fn window_discovery_matches_the_nearest_ancestor_client() {
        // conduct(pid 100) ← shell(same pid, exec) ← kitty(pid 42) ← hypr(pid 7).
        // kitty owns the window; the compositor (7) does not.
        let clients = vec![
            json!({ "pid": 42, "address": "0xKITTY", "class": "kitty" }),
            json!({ "pid": 999, "address": "0xOTHER", "class": "firefox" }),
        ];
        let ancestry = vec![100, 42, 7];
        assert_eq!(
            match_window_for_ancestry(&ancestry, &clients),
            Some("0xKITTY".to_string())
        );

        // The NEAREST ancestor with a window wins (self before its parents), even
        // if a further-up ancestor also owns a client.
        let nested = vec![
            json!({ "pid": 42, "address": "0xOUTER" }),
            json!({ "pid": 100, "address": "0xINNER" }),
        ];
        assert_eq!(
            match_window_for_ancestry(&[100, 42, 7], &nested),
            Some("0xINNER".to_string())
        );

        // No ancestor owns a window → None (address stays unset, as before).
        assert_eq!(match_window_for_ancestry(&[100, 42, 7], &[json!({ "pid": 5, "address": "0xX" })]), None);
        // A client whose pid matches but whose address is empty/absent is skipped.
        assert_eq!(
            match_window_for_ancestry(&[42], &[json!({ "pid": 42, "address": "" })]),
            None
        );
        assert_eq!(
            match_window_for_ancestry(&[42], &[json!({ "pid": 42, "class": "kitty" })]),
            None
        );
        // Empty inputs never match.
        assert_eq!(match_window_for_ancestry(&[], &clients), None);
        assert_eq!(match_window_for_ancestry(&ancestry, &[]), None);
    }

    #[test]
    fn window_discovery_also_returns_the_owning_pid() {
        // The hook door records the matched terminal pid (helps the reaper): a
        // `/proc` liveness signal that vanishes with the window, never a false
        // reap. The pid returned is the matched ancestor/client pid.
        let clients = vec![
            json!({ "pid": 42, "address": "0xKITTY", "class": "kitty" }),
            json!({ "pid": 999, "address": "0xOTHER" }),
        ];
        assert_eq!(
            match_window_and_pid(&[100, 42, 7], &clients),
            Some(("0xKITTY".to_string(), 42))
        );
        // Nearest ancestor wins, and its pid comes back with it.
        let nested = vec![
            json!({ "pid": 42, "address": "0xOUTER" }),
            json!({ "pid": 100, "address": "0xINNER" }),
        ];
        assert_eq!(
            match_window_and_pid(&[100, 42, 7], &nested),
            Some(("0xINNER".to_string(), 100))
        );
        // No match → None (address AND pid stay unset, never a partial record).
        assert_eq!(match_window_and_pid(&[5], &clients), None);
        assert_eq!(match_window_and_pid(&[42], &[json!({ "pid": 42, "address": "" })]), None);
    }

    #[test]
    fn client_workspace_lookup_reads_workspace_id_and_tolerates_address_form() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        // `hyprctl clients -j` carries `workspace: { id, name }`; the lookup pulls
        // the numeric id for the matching window. Address match is 0x/case
        // tolerant, exactly like window_present (the stored addr may differ).
        let clients = vec![
            json!({ "address": "0x55aabb", "workspace": { "id": 3, "name": "3" } }),
            json!({ "address": "0x1234ef", "workspace": { "id": 7, "name": "seven" } }),
            // Special workspaces carry NEGATIVE ids — surfaced verbatim.
            json!({ "address": "0xdeadbe", "workspace": { "id": -99, "name": "special:magic" } }),
        ];
        assert_eq!(client_workspace_for_address(&clients, "0x55aabb"), Some(3));
        assert_eq!(client_workspace_for_address(&clients, "55AABB"), Some(3)); // no 0x + upper
        assert_eq!(client_workspace_for_address(&clients, "0X1234EF"), Some(7));
        assert_eq!(client_workspace_for_address(&clients, "0xdeadbe"), Some(-99));

        // A window absent from the list → None (leave the stored workspace be).
        assert_eq!(client_workspace_for_address(&clients, "0xnope"), None);
        // An empty address never matches.
        assert_eq!(client_workspace_for_address(&clients, ""), None);
        // A client without a workspace object (or without a numeric id) → None,
        // never a panic — degrade gracefully when Hyprland omits the field.
        let noworkspace = vec![
            json!({ "address": "0xaa" }),
            json!({ "address": "0xbb", "workspace": {} }),
            json!({ "address": "0xcc", "workspace": { "name": "3" } }),
        ];
        assert_eq!(client_workspace_for_address(&noworkspace, "0xaa"), None);
        assert_eq!(client_workspace_for_address(&noworkspace, "0xbb"), None);
        assert_eq!(client_workspace_for_address(&noworkspace, "0xcc"), None);
    }

    #[test]
    fn hypr_event_parses_window_lifecycle_and_ignores_the_rest() {
        // openwindow>>ADDR,WORKSPACE,CLASS,TITLE — address is the first field
        // (Hyprland emits it WITHOUT the `0x` prefix; a title may contain commas).
        assert_eq!(
            parse_hypr_window_event("openwindow>>55aabbccdd00,1,kitty,shell — /home/x, y"),
            Some(HyprWindowEvent::Appeared {
                address: "55aabbccdd00".to_string()
            })
        );
        // A move (movewindow, movewindowv2) and a retitle (windowtitle, windowtitlev2) change a known window.
        assert_eq!(
            parse_hypr_window_event("movewindow>>55aabb,2"),
            Some(HyprWindowEvent::Changed {
                address: "55aabb".to_string()
            })
        );
        assert_eq!(
            parse_hypr_window_event("movewindowv2>>55aabb,2,two"),
            Some(HyprWindowEvent::Changed {
                address: "55aabb".to_string()
            })
        );
        assert_eq!(
            parse_hypr_window_event("windowtitle>>55aabb"),
            Some(HyprWindowEvent::Changed {
                address: "55aabb".to_string()
            })
        );
        assert_eq!(
            parse_hypr_window_event("windowtitlev2>>55aabb,a new title"),
            Some(HyprWindowEvent::Changed {
                address: "55aabb".to_string()
            })
        );
        // closewindow>>ADDR — the whole payload is the address.
        assert_eq!(
            parse_hypr_window_event("closewindow>>55aabb"),
            Some(HyprWindowEvent::Closed {
                address: "55aabb".to_string()
            })
        );
        // Events we don't act on → None.
        assert_eq!(parse_hypr_window_event("workspace>>2"), None);
        assert_eq!(parse_hypr_window_event("activewindow>>kitty,shell"), None);
        assert_eq!(parse_hypr_window_event("focusedmon>>DP-1,2"), None);
        // Malformed / empty-address lines → None (never a panic, never a blank).
        assert_eq!(parse_hypr_window_event("no-delimiter-here"), None);
        assert_eq!(parse_hypr_window_event("openwindow>>"), None);
        assert_eq!(parse_hypr_window_event("openwindow>> ,1,kitty,t"), None);
        assert_eq!(parse_hypr_window_event("closewindow>>   "), None);
        assert_eq!(parse_hypr_window_event(""), None);
    }

    #[test]
    fn hypr_event_socket_path_needs_a_signature() {
        // `hypr_event_socket_path()` reads process-global env; serialise it
        // against the other env-touching tests with the crate-wide lock.
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        let saved_rt = std::env::var("XDG_RUNTIME_DIR").ok();

        // No signature → no socket (listener disables itself off-Hyprland).
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        assert_eq!(hypr_event_socket_path(), None);

        // A blank signature is treated as absent, not as a path segment.
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "   ");
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        assert_eq!(hypr_event_socket_path(), None);

        // Signature + runtime dir → the documented socket2 path.
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "abc123_99");
        assert_eq!(
            hypr_event_socket_path(),
            Some(PathBuf::from(
                "/run/user/1000/hypr/abc123_99/.socket2.sock"
            ))
        );

        match saved_sig {
            Some(v) => std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v),
            None => std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
        match saved_rt {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
    }

    #[test]
    fn focus_window_rejects_an_empty_address_without_touching_hyprctl() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        // The shared focus fn is called by the socket loop on arbitrary input;
        // an empty/blank address is a structured error, never a panic and never
        // a stray `hyprctl` dispatch.
        let err = focus_window("").expect_err("empty address must error");
        assert_eq!(err.reason, "no-window-address");
        let err = focus_window("   ").expect_err("blank address must error");
        assert_eq!(err.reason, "no-window-address");
    }

    #[test]
    fn focus_window_never_succeeds_for_a_nonexistent_window() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        // A bogus address is never a live client, so the verify step fails —
        // either `window-not-found` (hyprctl present) or `hyprctl-unavailable`
        // (no compositor / hyprctl absent, e.g. the build sandbox). Both are
        // structured Errs: the fn must NEVER report a false focus and NEVER
        // panic, whatever the environment.
        let err = focus_window("0xdeadbeefcafe").expect_err("bogus address must not focus");
        assert!(
            matches!(
                err.reason,
                "window-not-found" | "hyprctl-unavailable" | "hyprctl-failed"
            ),
            "unexpected reason {}",
            err.reason
        );
    }

    /// `resolve_pending_session_windows`'s SKIP decision for a headless
    /// wrap's own record is `windowless_by_lineage`'s self-check (proven
    /// directly above) — the listener function itself is deliberately NOT
    /// exercised end-to-end here: it early-returns the moment
    /// `hyprctl_clients()` fails (no `HYPRLAND_INSTANCE_SIGNATURE` in a
    /// CI/test environment), so the mutating loop that consults the
    /// `windowless` set never even runs off-Hyprland. This test pins the
    /// one thing that IS honestly testable without a compositor: the
    /// function is a safe, total no-op — never a panic, never a change —
    /// on a roster that includes a headless wrap when Hyprland is absent.
    /// The client list comes off the request socket, never a `hyprctl` binary: with
    /// PATH pointed at an empty directory (the daemon's service PATH carries no
    /// `hyprctl`), the read still answers the window the socket holds.
    #[cfg(unix)]
    #[test]
    fn hyprctl_clients_reads_the_request_socket_with_no_hyprctl_on_path() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = EnvVars::save(&["PATH"]);
        let empty = unique_stage("socket-no-path");
        std::env::set_var("PATH", &empty);
        assert!(std::process::Command::new("hyprctl").output().is_err(), "no hyprctl on this PATH");

        let (socket, dir) = fake_hypr_socket("clients-socket");
        let clients = hyprctl_clients().expect("the socket answers j/clients");
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["address"], "0xAA");
        assert_eq!(clients[0]["workspace"]["id"], 3);
        drop(socket);
        drop(saved);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    /// A request socket bound at `<root>/hypr/test/.socket.sock`, with `XDG_RUNTIME_DIR=<root>`
    /// and `HYPRLAND_INSTANCE_SIGNATURE=test` set. The caller owns the listener and the env.
    #[cfg(unix)]
    fn bind_request_socket(tag: &str) -> (EnvVars, PathBuf, std::os::unix::net::UnixListener) {
        let root = unique_stage(tag);
        let dir = root.join("hypr").join("test");
        std::fs::create_dir_all(&dir).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(dir.join(".socket.sock")).unwrap();
        let env = EnvVars::save(&["XDG_RUNTIME_DIR", "HYPRLAND_INSTANCE_SIGNATURE"]);
        std::env::set_var("XDG_RUNTIME_DIR", &root);
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "test");
        (env, root, listener)
    }

    /// A request socket that never accepts: its backlog fills, the next plain connect blocks
    /// in the kernel, and `hyprctl_clients` must still answer `None` within its budget.
    #[cfg(unix)]
    #[test]
    fn hyprctl_clients_gives_up_within_its_budget_when_the_backlog_is_full() {
        use std::os::unix::net::UnixStream;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::time::Duration;
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root, listener) = bind_request_socket("backlog-full");
        let socket = root.join("hypr").join("test").join(".socket.sock");

        // Connect until the kernel blocks one; nothing accepts, so the queue fills and stays full.
        let queued = Arc::new(AtomicUsize::new(0));
        {
            let queued = Arc::clone(&queued);
            std::thread::spawn(move || {
                let mut held = Vec::new();
                while let Ok(s) = UnixStream::connect(&socket) {
                    held.push(s);
                    queued.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
        let mut seen = queued.load(Ordering::SeqCst);
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let now = queued.load(Ordering::SeqCst);
            if now == seen {
                break;
            }
            seen = now;
        }
        assert!(seen > 0, "the filler never connected");

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(hyprctl_clients());
        });
        let answered = rx
            .recv_timeout(Duration::from_secs(3))
            .expect("hyprctl_clients was still blocked after 3 s on a full backlog");
        assert!(answered.is_none(), "a listener that never accepts answers nothing");
        drop(listener);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A reply that trickles in a byte at a time, each gap inside the per-read timeout, still
    /// outlasts the budget. It is a complete JSON array, so only the shared deadline can refuse it.
    #[cfg(unix)]
    #[test]
    fn hyprctl_clients_gives_up_on_a_reply_that_trickles_past_its_budget() {
        use std::io::{Read, Write};
        use std::time::{Duration, Instant};
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, root, listener) = bind_request_socket("trickle");
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut request = [0u8; 16];
            let _ = conn.read(&mut request);
            let _ = conn.write_all(b"[");
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(300));
                if conn.write_all(b" ").is_err() {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(300));
            let _ = conn.write_all(b"]");
        });
        let started = Instant::now();
        let answered = hyprctl_clients();
        let took = started.elapsed();
        assert!(answered.is_none(), "a reply cut off at the budget is no reply");
        assert!(took < Duration::from_secs(3), "the reply read ran {took:?}");
        let _ = server.join();
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The sweep decides when core is asked to bind: once for a pending window, once more for each real
    /// move to another workspace, and not for an observation core already holds. The stamp itself is
    /// core's (its own tests cover it), so the stub core records the calls the sweep makes.
    #[cfg(unix)]
    #[test]
    fn the_sweep_asks_core_to_bind_a_pending_window_and_again_only_on_a_move() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sweep-asks-core");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let (_core, core_root) = stub_core_door("sweep-asks-core");
        let mut rec = session("s1", "/w", "working", "t", None);
        rec.window_address = String::new();
        rec.pid = Some(std::process::id());
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions: vec![rec] },
        )
        .unwrap();

        let (env, dir) = fake_hypr_socket("sweep-asks-core");
        assert!(resolve_pending_session_windows(), "the pending window binds through core");
        assert_eq!(core_calls(&core_root), ["session window attach s1 0xAA --workspace 3 --json"]);
        record_binding("s1", "0xAA", Some(3));

        // Dragged to another bound workspace: core is asked for the new workspace, once.
        std::env::set_var("AOIDE_TEST_WS", "7");
        assert!(resolve_pending_session_windows());
        assert_eq!(
            core_calls(&core_root),
            [
                "session window attach s1 0xAA --workspace 3 --json",
                "session window attach s1 0xAA --workspace 7 --json",
            ]
        );
        record_binding("s1", "0xAA", Some(7));

        assert!(!resolve_pending_session_windows(), "idempotent once core holds the observation");
        assert_eq!(core_calls(&core_root).len(), 2);

        drop(env);
        let _ = std::fs::remove_dir_all(&dir);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// A decoded `hyprctl clients -j` entry: an app window owned by `pid`.
    fn client(addr: &str, pid: i64, class: &str) -> Value {
        json!({ "address": addr, "pid": pid, "class": class })
    }

    #[test]
    fn a_desktop_chain_binds_to_its_app_window_not_a_terminal() {
        // A harness (1137986) under a desktop app (867785) whose window's class is
        // not a terminal's. The owner is found by pid alone, whatever the class.
        let clients = [client("0x639050c25000", 867785, "com.example.App")];
        let chain = [119735, 1137986, 867785, 1454];
        let beneath = |p: i32, owner: i32| p == 1137986 && owner == 867785;
        assert_eq!(
            window_for_hook_chain(&chain, &clients, beneath).as_deref(),
            Some("0x639050c25000")
        );
    }

    #[test]
    fn a_stale_chain_with_nothing_beneath_its_owner_binds_nothing() {
        let clients = [client("0xAA", 867785, "com.example.App")];
        let chain = [2_000_000_000, 2_000_000_001, 867785, 1454];
        assert_eq!(window_for_hook_chain(&chain, &clients, |_, _| false), None);
    }

    #[test]
    fn a_chain_with_no_owner_client_binds_nothing() {
        let clients = [client("0xAA", 999, "com.example.App")];
        assert_eq!(window_for_hook_chain(&[1, 2, 3], &clients, |_, _| true), None);
    }

    #[test]
    fn the_nearest_owner_wins_over_a_farther_one() {
        let clients = [client("0xFAR", 900, "com.example.App"), client("0xNEAR", 500, "kitty")];
        let chain = [10, 500, 900];
        let beneath = |p: i32, owner: i32| p == 10 && owner == 500;
        assert_eq!(
            window_for_hook_chain(&chain, &clients, beneath).as_deref(),
            Some("0xNEAR")
        );
    }

    #[test]
    fn an_owner_at_the_head_of_the_chain_has_nothing_beneath_it() {
        let clients = [client("0xAA", 500, "kitty")];
        assert_eq!(window_for_hook_chain(&[500, 1], &clients, |_, _| true), None);
    }

    /// The sweep asks core to bind the pid-less hosted sessions whose hook chain runs under a live
    /// client, asks for none of the stale, done, windowless-by-lineage or chainless ones, and writes
    /// nothing itself. The real children stand in for harnesses beneath the app, which is this test
    /// process's own client.
    #[cfg(unix)]
    #[test]
    fn the_sweep_asks_core_to_bind_pid_less_hosted_sessions_by_their_hook_chain() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sweep-hook-chain");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let (_core, core_root) = stub_core_door("sweep-hook-chain");

        let me = std::process::id() as i32;
        let dead = 2_000_000_000_i32;
        let mut children: Vec<std::process::Child> = (0..3)
            .map(|_| std::process::Command::new("sleep").arg("30").spawn().unwrap())
            .collect();
        let kids: Vec<i32> = children.iter().map(|c| c.id() as i32).collect();

        let hosted = |id: &str, chain: Vec<i32>| {
            let mut r = session(id, "/w", "working", "t", None);
            r.window_address = String::new();
            r.hook_ancestry = chain;
            r
        };
        let mut recs: Vec<SessionRecord> = kids
            .iter()
            .enumerate()
            .map(|(i, &k)| hosted(&format!("h{}", i + 1), vec![dead, dead + 1, k, me]))
            .collect();
        recs.push(hosted("dead", vec![dead, dead + 1, dead + 2, me]));
        let mut done = hosted("done", vec![kids[0], me]);
        done.state = "done".into();
        recs.push(done);
        let mut wrap = session("wrap", "/w", "idle", "t", None);
        wrap.window_address = String::new();
        wrap.conductable = Some(true);
        wrap.headless = true;
        recs.push(wrap);
        let mut nested = hosted("nested", vec![kids[0], me]);
        nested.parent_session_id = Some("wrap".into());
        recs.push(nested);
        let mut chainless = session("chainless", "/w", "working", "t", None);
        chainless.window_address = String::new();
        recs.push(chainless);
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions: recs },
        )
        .unwrap();
        let before = std::fs::read(sessions_path()).unwrap();

        let (env, dir) = fake_hypr_socket("sweep-hook-chain");
        std::env::set_var("AOIDE_TEST_CLIENT_PID", me.to_string());
        std::env::set_var("AOIDE_TEST_CLIENT_CLASS", "com.example.App");
        assert!(resolve_pending_session_windows(), "the hook-chain sessions bind through core");

        let calls = core_calls(&core_root);
        for id in ["h1", "h2", "h3"] {
            let ask = format!("session window attach {id} 0xAA --workspace 3 --json");
            assert!(calls.contains(&ask), "{id} binds through its live chain: {calls:?}");
        }
        assert_eq!(calls.len(), 3, "and nothing else is asked: {calls:?}");
        assert_eq!(
            std::fs::read(sessions_path()).unwrap(),
            before,
            "the desktop never writes sessions.json itself"
        );

        for c in children.iter_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        drop(env);
        let _ = std::fs::remove_dir_all(&dir);
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    /// The host WITHOUT a compositor — the arm native Windows runs, and the one
    /// the adapter's whole "no adapter" posture rests on: an unset
    /// `HYPRLAND_INSTANCE_SIGNATURE` means no guess is ever made.
    #[test]
    fn focused_workspace_refuses_a_host_without_a_compositor() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        assert_eq!(focused_workspace(), None, "off-Hyprland: no guess, ever");
        match saved_sig {
            Some(v) => std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v),
            None => std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
    }

    /// The compositor-PRESENT half of that pair — GATED on Unix with its
    /// reason: the fixture is a `#!/bin/sh` shim (`testutil::fake_hyprctl`)
    /// that `CreateProcess` cannot launch at all (no shebang, no extension),
    /// and the subject is a compositor native Windows does not have — the same
    /// gate the client crate's shim-driven groups carry.
    #[cfg(unix)]
    #[test]
    fn focused_workspace_reads_the_compositor() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        assert_eq!(focused_workspace(), None, "off-Hyprland: no guess, ever");

        let (env, dir) = fake_hyprctl("focused");
        std::env::set_var("AOIDE_TEST_WS", "5");
        assert_eq!(focused_workspace(), Some(5), "the shim reports workspace 5");
        std::env::set_var("AOIDE_TEST_WS", "-99");
        assert_eq!(
            focused_workspace(),
            Some(-99),
            "a special workspace's negative id is surfaced verbatim"
        );

        drop(env);
        let _ = std::fs::remove_dir_all(&dir);
        match saved_sig {
            Some(v) => std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v),
            None => std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
    }

    #[test]
    fn resolve_pending_session_windows_is_a_safe_noop_off_hyprland() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("resolve-pending-noop");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        let mut wrap = session("wrap", "/w", "working", "1", None);
        wrap.conductable = Some(true);
        wrap.headless = true;
        wrap.window_address = String::new();
        wrap.pid = Some(999999);
        let file = SessionsFile {
            schema_version: "0".into(),
            sessions: vec![wrap],
        };
        write_stage(&sessions_path(), &file).unwrap();

        assert!(
            !resolve_pending_session_windows(),
            "no Hyprland signature → hyprctl_clients() fails → an honest false, never a stray write"
        );
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(
            after.sessions[0].window_address, "",
            "the headless wrap's record is untouched"
        );

        match saved_sig {
            Some(v) => std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v),
            None => std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

// -- The owed calls: each is held to what it is owed --
//
// A closed window is reported once per event, the codex sync runs on an appearance or a
// reconnect, the roster goes out when its content or its claims change, a call core answers is
// settled, a call that did not run is tried on the next tick, and with no aoide binary every call is
// skipped with one log line.

/// A closed window is reported once, and not again once core has answered.
#[cfg(unix)]
#[test]
fn a_closed_window_is_reported_once_and_not_again_once_core_answers() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let (_core, core_root) = stub_core_door("owed-closed-once");
    let mut owed = Owed::default();
    owed.window_closed("55aabb".into());
    owed.settle_calls();
    owed.settle_calls();
    assert_eq!(core_calls(&core_root), ["session window closed 55aabb --json"]);
}

/// A refused close is final: core answered, so the close is not sent again on a tick, and the refusal
/// is logged once.
#[cfg(unix)]
#[test]
fn a_refused_close_is_settled_and_not_sent_again() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let _stage = EnvVars::save(&["AOIDE_STAGE_DIR"]);
    let stage = unique_stage("owed-closed-refused");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let (_core, core_root) = stub_core_door("owed-closed-refused");
    std::fs::write(core_root.join("refuse"), "").unwrap();
    let mut owed = Owed::default();
    owed.window_closed("55aabb".into());
    owed.settle_calls();
    owed.settle_calls();
    owed.tick();
    owed.tick();
    assert_eq!(core_calls(&core_root), ["session window closed 55aabb --json"], "sent once, however many ticks follow");
    assert_eq!(owed.logged(), 1, "the refusal is logged once");
    std::fs::remove_file(core_root.join("refuse")).unwrap();
    owed.tick();
    assert_eq!(core_calls(&core_root).len(), 1, "a settled close is not sent again, even once core would accept it");
    let _ = std::fs::remove_dir_all(&stage);
}

/// A refused codex sync is final: the appearance that owed it is sent once, and idle ticks send nothing.
#[cfg(unix)]
#[test]
fn a_refused_codex_sync_is_not_sent_again() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let (_core, core_root) = stub_core_door("owed-codex-refused");
    std::fs::write(core_root.join("refuse"), "").unwrap();
    let mut owed = Owed::default();
    owed.event(HyprWindowEvent::Appeared { address: "55aabb".into() });
    owed.tick();
    owed.tick();
    assert_eq!(core_calls(&core_root), ["session codex sync --json"], "a refused sync is sent once");
    std::fs::remove_file(core_root.join("refuse")).unwrap();
    owed.tick();
    assert_eq!(core_calls(&core_root).len(), 1, "a settled sync is not sent again");
}

/// The codex sync runs when a window appears and on each reconnect, and not on a tick in between.
#[cfg(unix)]
#[test]
fn the_codex_sync_is_owed_on_an_appearance_and_a_reconnect_and_never_on_a_tick() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let (_core, core_root) = stub_core_door("owed-codex");
    let mut owed = Owed::default();
    owed.settle_calls();
    owed.tick();
    assert!(core_calls(&core_root).is_empty(), "nothing owed, nothing sent");
    owed.event(HyprWindowEvent::Appeared { address: "55aabb".into() });
    owed.tick();
    assert_eq!(core_calls(&core_root), ["session codex sync --json"]);
    owed.reconnected();
    owed.settle_calls();
    owed.tick();
    assert_eq!(core_calls(&core_root), ["session codex sync --json"; 2]);
}

/// The roster goes out once when it first exists, then when the terminal windows or the sessions'
/// claims change, and again on each reconnect.
#[cfg(unix)]
#[test]
fn the_roster_goes_out_on_a_change_and_on_a_reconnect_and_never_on_an_unchanged_tick() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_TEST_CLIENT_CLASS"]);
    let stage = unique_stage("owed-roster");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let (_core, core_root) = stub_core_door("owed-roster");
    let (_hypr, _hypr_root) = fake_hypr_socket("owed-roster");
    std::env::set_var("AOIDE_TEST_CLIENT_CLASS", "kitty");
    let mut owed = Owed::default();

    owed.settle_roster();
    owed.settle_roster();
    assert_eq!(core_calls(&core_root).len(), 1, "the first roster goes out, an unchanged one does not");

    write_stage(
        &sessions_path(),
        &SessionsFile {
            schema_version: "0".into(),
            sessions: vec![SessionRecord { window_address: "0xAA".into(), ..session("claimant", "/w", "working", "t", None) }],
        },
    )
    .unwrap();
    owed.settle_roster();
    assert_eq!(core_calls(&core_root).len(), 2, "a session claiming the window changes the roster");
    owed.settle_roster();
    assert_eq!(core_calls(&core_root).len(), 2);

    std::env::set_var("AOIDE_TEST_CLIENT_CLASS", "firefox");
    owed.settle_roster();
    assert_eq!(core_calls(&core_root).len(), 3, "a window that is no longer a terminal changes it");

    owed.reconnected();
    owed.settle_roster();
    assert_eq!(core_calls(&core_root).len(), 4, "a reconnect sends the roster again");
    let _ = std::fs::remove_dir_all(&stage);
}

/// A refused roster is final: idle ticks send nothing more, and a changed roster is sent once.
#[cfg(unix)]
#[test]
fn a_refused_roster_is_not_sent_again_until_the_roster_changes() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_TEST_CLIENT_CLASS"]);
    let stage = unique_stage("owed-roster-refused");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let (_core, core_root) = stub_core_door("owed-roster-refused");
    let (_hypr, _hypr_root) = fake_hypr_socket("owed-roster-refused");
    std::env::set_var("AOIDE_TEST_CLIENT_CLASS", "kitty");
    std::fs::write(core_root.join("refuse"), "").unwrap();
    let mut owed = Owed::default();
    owed.settle_roster();
    assert_eq!(core_calls(&core_root).len(), 1, "a refused roster is sent once");
    for _ in 0..10 {
        owed.tick();
    }
    assert_eq!(core_calls(&core_root).len(), 1, "idle ticks send nothing more");
    std::env::set_var("AOIDE_TEST_CLIENT_CLASS", "firefox");
    owed.tick();
    assert_eq!(core_calls(&core_root).len(), 2, "a changed roster is sent once");
    owed.tick();
    assert_eq!(core_calls(&core_root).len(), 2, "and not again while it stays refused");
    let _ = std::fs::remove_dir_all(&stage);
}

/// Without an aoide binary every owed call is skipped: none errors, and each is logged once however
/// many ticks pass.
#[cfg(unix)]
#[test]
fn with_no_aoide_binary_each_owed_call_is_skipped_and_logged_once() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _core = EnvVars::save(&["AOIDE_CORE_BIN", "AOIDE_STAGE_DIR"]);
    std::env::set_var("AOIDE_CORE_BIN", "/nonexistent/aoide");
    let stage = unique_stage("owed-no-binary");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let (_hypr, _hypr_root) = fake_hypr_socket("owed-no-binary");
    let mut owed = Owed::default();
    owed.reconnected();
    owed.window_closed("55aabb".into());
    owed.settle_calls();
    owed.tick();
    owed.tick();
    assert_eq!(owed.logged(), 3, "the codex sync, the closed window and the roster, each once");
    let _ = std::fs::remove_dir_all(&stage);
}

/// A core binary that is present but cannot exec did not run: the door reports NotRun, so the call stays
/// owed and a later tick tries again. It is not skipped as a missing binary is.
#[cfg(unix)]
#[test]
fn a_binary_that_cannot_exec_is_a_call_that_did_not_run_not_a_skipped_one() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _daemon = core_up();
    let dir = aoide_test_support::unique_tmp("unexecutable");
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("aoide");
    std::fs::write(&bin, "not a program").unwrap();
    let _core = EnvVars::save(&["AOIDE_CORE_BIN"]);
    std::env::set_var("AOIDE_CORE_BIN", &bin);
    let reply = super::door::call(&["session", "codex", "sync"], None);
    assert_eq!(reply, super::door::Reply::NotRun);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A call whose exec failed did not run: it stays owed and is logged once, and the next tick sends it
/// once the exec works.
#[cfg(unix)]
#[test]
fn a_call_that_did_not_run_is_tried_again_on_the_next_tick() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let (_core, core_root) = stub_core_door("owed-not-run");
    let stub = core_root.join("stub-aoide");
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
    let mut owed = Owed::default();
    owed.window_closed("55aabb".into());
    owed.settle_calls();
    owed.tick();
    assert!(core_calls(&core_root).is_empty(), "a stub that cannot exec runs nothing");
    assert_eq!(owed.logged(), 1, "the failed exec is logged once");
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    owed.tick();
    assert_eq!(core_calls(&core_root), ["session window closed 55aabb --json"], "the next tick tries it again");
    assert_eq!(owed.logged(), 0, "an accepted call clears its latch");
}

/// Without an aoide binary the sweep binds nothing, reports false, and logs the refused bind once.
#[cfg(unix)]
#[test]
fn with_no_aoide_binary_the_sweep_binds_nothing_and_logs_once() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_CORE_BIN"]);
    let stage = unique_stage("sweep-no-binary");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    std::env::set_var("AOIDE_CORE_BIN", "/nonexistent/aoide");
    let mut rec = session("s1", "/w", "working", "t", None);
    rec.window_address = String::new();
    rec.pid = Some(std::process::id());
    write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions: vec![rec] }).unwrap();
    let (_hypr, _hypr_root) = fake_hypr_socket("sweep-no-binary");

    let mut latch = Latch::default();
    assert!(!resolve_windows(&mut latch));
    assert!(!resolve_windows(&mut latch));
    assert_eq!(latch.logged(), 1, "the skipped bind is logged once");
    let after: SessionsFile = load_stage(&sessions_path()).unwrap();
    assert_eq!(after.sessions[0].window_address, "", "nothing was bound");
    let _ = std::fs::remove_dir_all(&stage);
}

/// A bind core refuses is final for its session, window and workspace: no later sweep asks for it again,
/// and a move to another workspace is a new binding, which is asked once.
#[cfg(unix)]
#[test]
fn a_refused_bind_is_settled_and_a_new_workspace_is_asked() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_TEST_WS"]);
    let stage = unique_stage("sweep-refused");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let (_core, core_root) = stub_core_door("sweep-refused");
    let mut rec = session("s1", "/w", "working", "t", None);
    rec.window_address = String::new();
    rec.pid = Some(std::process::id());
    write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions: vec![rec] }).unwrap();
    let (_hypr, _hypr_root) = fake_hypr_socket("sweep-refused");
    std::fs::write(core_root.join("refuse"), "").unwrap();

    let mut latch = Latch::default();
    assert!(!resolve_windows(&mut latch));
    assert!(!resolve_windows(&mut latch));
    assert_eq!(core_calls(&core_root), ["session window attach s1 0xAA --workspace 3 --json"], "a refused bind is sent once");
    assert_eq!(latch.logged(), 1, "and logged once");
    std::fs::remove_file(core_root.join("refuse")).unwrap();
    assert!(!resolve_windows(&mut latch), "the refused binding is not sent again, even once core would accept it");
    assert_eq!(core_calls(&core_root).len(), 1);

    std::env::set_var("AOIDE_TEST_WS", "7");
    assert!(resolve_windows(&mut latch), "the new workspace is a new binding, and core accepts it");
    assert_eq!(core_calls(&core_root)[1], "session window attach s1 0xAA --workspace 7 --json");
    assert_eq!(latch.logged(), 0, "an accepted call clears its latch");
    let _ = std::fs::remove_dir_all(&stage);
}

/// A bind that aoided did not take is not lost to a quiet stretch: the sweep runs on each tick, so the
/// bind goes out on the first tick after aoided listens.
#[cfg(unix)]
#[test]
fn an_unsent_bind_goes_out_on_a_later_tick() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _env = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET"]);
    let stage = unique_stage("owed-bind-tick");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let (_core, core_root) = stub_core_door("owed-bind-tick");
    let (_hypr, _hypr_root) = fake_hypr_socket("owed-bind-tick");
    let mut rec = session("s1", "/w", "working", "t", None);
    rec.window_address = String::new();
    rec.pid = Some(std::process::id());
    write_stage(&sessions_path(), &SessionsFile { schema_version: "0".into(), sessions: vec![rec] }).unwrap();
    std::env::set_var("AOIDE_DAEMON_SOCKET", "/nonexistent/aoided-is-down.sock");
    let mut owed = Owed::default();
    owed.tick();
    assert!(core_calls(&core_root).is_empty(), "no exec while aoided is down");
    std::env::set_var("AOIDE_DAEMON_SOCKET", accepting_daemon());
    owed.tick();
    assert!(
        core_calls(&core_root).iter().any(|call| call == "session window attach s1 0xAA --workspace 3 --json"),
        "the bind goes out on the first tick after aoided listens: {:?}",
        core_calls(&core_root)
    );
    let _ = std::fs::remove_dir_all(&stage);
}

/// With aoided not listening nothing is execed: the owed close waits, logged once, and goes out on the
/// first tick after aoided listens.
#[cfg(unix)]
#[test]
fn an_absent_aoided_gets_no_call_until_it_listens() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let (_core, core_root) = stub_core_door("owed-absent");
    let _stage = EnvVars::save(&["AOIDE_STAGE_DIR"]);
    let stage = unique_stage("owed-absent");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    std::env::set_var("AOIDE_DAEMON_SOCKET", "/nonexistent/aoided-is-down.sock");
    let mut owed = Owed::default();
    owed.event(HyprWindowEvent::Closed { address: "55aabb".into() });
    owed.tick();
    owed.tick();
    assert!(core_calls(&core_root).is_empty(), "no exec while aoided is down");
    assert_eq!(owed.logged(), 1, "the wait is logged once");
    std::env::set_var("AOIDE_DAEMON_SOCKET", accepting_daemon());
    owed.tick();
    owed.tick();
    assert_eq!(core_calls(&core_root), ["session window closed 55aabb --json"], "sent once, once aoided listens");
    assert_eq!(owed.logged(), 0, "an accepted call clears its latch");
    let _ = std::fs::remove_dir_all(&stage);
}

/// A socket file a dead aoided left behind refuses connections, so it counts as absent.
#[cfg(unix)]
#[test]
fn a_stale_daemon_socket_counts_as_absent() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let (_core, core_root) = stub_core_door("owed-stale");
    let _stage = EnvVars::save(&["AOIDE_STAGE_DIR"]);
    let stage = unique_stage("owed-stale");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let dir = aoide_test_support::unique_tmp("stale");
    std::fs::create_dir_all(&dir).unwrap();
    let stale = dir.join("aoided.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    std::env::set_var("AOIDE_DAEMON_SOCKET", &stale);
    let mut owed = Owed::default();
    owed.event(HyprWindowEvent::Closed { address: "55aabb".into() });
    assert!(core_calls(&core_root).is_empty(), "a refused connect is absent, not a daemon");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&stage);
}

/// An appearance owes the codex sync: a move and a retitle reconcile the sweep and the roster.
#[cfg(unix)]
#[test]
fn a_retitle_owes_no_codex_sync_and_an_appearance_does() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _sig = no_compositor();
    let (_core, core_root) = stub_core_door("owed-retitle");
    let _stage = EnvVars::save(&["AOIDE_STAGE_DIR"]);
    let stage = unique_stage("owed-retitle");
    std::env::set_var("AOIDE_STAGE_DIR", &stage);
    let mut owed = Owed::default();
    for line in ["windowtitle>>55aabb", "windowtitlev2>>55aabb,new title", "movewindow>>55aabb,2"] {
        owed.event(parse_hypr_window_event(line).expect("a window event"));
    }
    assert!(core_calls(&core_root).is_empty(), "a retitle or a move owes core nothing");
    owed.event(parse_hypr_window_event("openwindow>>55aabb,1,kitty,t").expect("a window event"));
    assert_eq!(core_calls(&core_root), ["session codex sync --json"]);
    let _ = std::fs::remove_dir_all(&stage);
}
