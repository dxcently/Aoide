//! The desktop's Hyprland tests: the request socket, the focus verbs, the event listener, the
//! terminal roster, and the session-window sweep, each against a fake request socket or a
//! fake `hyprctl` shim, never a live compositor.

use super::client::*;
use super::focus::*;
use super::listener::*;
use super::sweep::*;
use super::terminals::*;
use super::testutil::*;
use aoide_conduct::graph::{
    attach_window, load_stage, normalize_addr, sessions_path, write_stage, Attach, Project,
    ProjectsFile, SessionRecord, SessionsFile,
};
use aoide_storage::stage::projects_path;
use aoide_test_support::fake_hyprctl;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The sweep as the desktop runs it, with the bind written in-process (the same library call the
/// core door makes) rather than exec'd, so these tests need no built `aoide`.
fn resolve_pending_session_windows() -> bool {
    let Some(clients) = hyprctl_clients() else {
        return false;
    };
    let Ok(file) = load_stage::<SessionsFile>(&sessions_path()) else {
        return false;
    };
    resolve_windows_in(&file.sessions, &clients, &mut |id, window, ws| {
        matches!(attach_window(id, window, ws), Attach::Bound { changed: true, .. })
    })
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
        // movewindow / movewindowv2 re-check (session may have registered late).
        assert_eq!(
            parse_hypr_window_event("movewindow>>55aabb,2"),
            Some(HyprWindowEvent::Appeared {
                address: "55aabb".to_string()
            })
        );
        assert_eq!(
            parse_hypr_window_event("movewindowv2>>55aabb,2,two"),
            Some(HyprWindowEvent::Appeared {
                address: "55aabb".to_string()
            })
        );
        // windowtitle (old, ADDR only) and windowtitlev2 (ADDR,TITLE).
        assert_eq!(
            parse_hypr_window_event("windowtitle>>55aabb"),
            Some(HyprWindowEvent::Appeared {
                address: "55aabb".to_string()
            })
        );
        assert_eq!(
            parse_hypr_window_event("windowtitlev2>>55aabb,a new title"),
            Some(HyprWindowEvent::Appeared {
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

    /// GATED on Unix with its reason (see `testutil::fake_hyprctl`): the
    /// fixture is a `#!/bin/sh` shim `CreateProcess` cannot launch, over a
    /// compositor native Windows does not run.
    #[cfg(unix)]
    #[test]
    fn the_sweep_stamps_the_default_at_birth_and_never_on_a_move() {
        // The compositor stamp site, end to end: the sweep resolves our own
        // pid to the shim's client, observes its workspace through the ONE
        // seam, and the birth default lands — once, and only once.
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sweep-stamps-default");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        write_stage(
            &projects_path(),
            &ProjectsFile {
                schema_version: "0".into(),
                projects: vec![
                    Project { name: "aoide".into(), workspaces: vec![3], ..Default::default() },
                    Project { name: "cadenza".into(), workspaces: vec![7], ..Default::default() },
                ],
            },
        )
        .unwrap();
        let mut rec = session("s1", "/w", "working", "t", None);
        rec.window_address = String::new();
        rec.pid = Some(std::process::id());
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions: vec![rec] },
        )
        .unwrap();

        let (env, dir) = fake_hypr_socket("sweep-stamp");
        assert!(
            resolve_pending_session_windows(),
            "the pending window resolves against the shim"
        );
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(after.sessions[0].window_address, "0xAA");
        assert_eq!(after.sessions[0].workspace, Some(3));
        assert_eq!(
            after.sessions[0].workspace_project.as_deref(),
            Some("aoide"),
            "born on workspace 3 → the bound project"
        );

        // Dragged to another BOUND workspace: the id follows, the default holds.
        std::env::set_var("AOIDE_TEST_WS", "7");
        assert!(resolve_pending_session_windows());
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        assert_eq!(after.sessions[0].workspace, Some(7));
        assert_eq!(after.sessions[0].workspace_project.as_deref(), Some("aoide"));

        // Nothing left to write once the observation is already current.
        assert!(!resolve_pending_session_windows(), "idempotent once settled");

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

    /// The sweep binds pid-less hosted sessions through their hook chain, binds
    /// none of the stale, done, windowless-by-lineage or chainless ones, and
    /// later retires only the one whose process is gone. The real children stand
    /// in for harnesses beneath the app, which is this test process's own client.
    #[cfg(unix)]
    #[test]
    fn the_sweep_binds_pid_less_hosted_sessions_by_their_hook_chain() {
        let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("sweep-hook-chain");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

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

        let (env, dir) = fake_hypr_socket("sweep-hook-chain");
        std::env::set_var("AOIDE_TEST_CLIENT_PID", me.to_string());
        std::env::set_var("AOIDE_TEST_CLIENT_CLASS", "com.example.App");
        assert!(resolve_pending_session_windows(), "the hook-chain sessions bind");

        let by_id = |s: &SessionsFile, id: &str| {
            s.sessions.iter().find(|r| r.session_id == id).unwrap().clone()
        };
        let after: SessionsFile = load_stage(&sessions_path()).unwrap();
        for id in ["h1", "h2", "h3"] {
            let r = by_id(&after, id);
            assert_eq!(r.window_address, "0xAA", "{id} binds through its live chain");
            assert_eq!(r.workspace, Some(3));
            assert_eq!(r.pid, None, "the sweep never gives a pid-less session a pid");
        }
        for id in ["dead", "done", "nested", "chainless", "wrap"] {
            assert!(by_id(&after, id).window_address.is_empty(), "{id} stays unbound");
        }

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
