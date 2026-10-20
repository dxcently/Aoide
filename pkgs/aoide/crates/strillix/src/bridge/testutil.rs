//! The strillix bridge's test rig: the stub binaries and roots its arms are driven
//! through, the short socket path the socket tests bind, and the env lock every
//! test that sets env takes.

use std::path::PathBuf;

/// The process-wide env lock. The floors match conduct's own lock: the first caller
/// points the daemon socket and the state dir away from anything live.
pub(crate) fn env_lock() -> &'static std::sync::Mutex<()> {
    static ISOLATE: std::sync::Once = std::sync::Once::new();
    ISOLATE.call_once(|| {
        if std::env::var("AOIDE_DAEMON_SOCKET").is_err() {
            std::env::set_var("AOIDE_DAEMON_SOCKET", "/nonexistent/strillix-tests-never-a-real-daemon.sock");
        }
        if std::env::var("AOIDE_STATE_DIR").is_err() {
            std::env::set_var(
                "AOIDE_STATE_DIR",
                std::env::temp_dir().join(format!("strillix-tests-state-{}", std::process::id())),
            );
        }
    });
    aoide_test_support::env_lock()
}

/// A socket path SHORT enough for this host's `sun_path` budget (see the core
/// accept-loop tests: native Windows caps an AF_UNIX path at 107 bytes).
pub(crate) fn short_sock(tag: &str) -> PathBuf {
    aoide_test_support::short_tmp(&format!("sb-{tag}")).join("s.sock")
}

/// A `#!/bin/sh` stand-in for `strillix`, a stage dir holding a marker for
/// `mode`, and the env that points both at them. It logs every argv it is
/// given beside itself, and answers the verbs the menu plans with
/// the envelopes the CLI prints: `list` echoes a fixed rices, `draft save` mints `draft-3`, `draft list`
/// lists two drafts, anything else succeeds. A `refuse-draft`,
/// `refuse-drop`, `refuse-list` or `refuse-rices` file beside it turns that verb into a refusal. NO notify
/// is ever reached — the tests drive the sequencer, not `handle_conn`.
///
/// GATED on Unix with its reason (its callers are gated with it): a
/// `#!/bin/sh` script made executable by a mode, which `CreateProcess`
/// launches neither by shebang nor by an extension-less name.
#[cfg(unix)]
pub(crate) fn stub_rice_bin(tag: &str, marker: &str) -> (aoide_test_support::EnvSaver, PathBuf) {
    let root = aoide_test_support::unique_tmp(tag);
    std::fs::create_dir_all(&root).unwrap();
    let stub = root.join("stub-strillix");
    std::fs::write(
        &stub,
        r#"#!/bin/sh
here="$(dirname "$0")"
echo "$*" >> "$here/argv.log"
case "$2 $3" in
"list --json")
  if [ -f "$here/refuse-rices" ]; then
printf '{"status":"error","message":"rices unreadable","data":{"reason":"x"}}\n'; exit 1
  fi
  printf '{"status":"ok","message":"2 rice(s)","data":{"mode":"declarative","rice":"sonata","draft":null,"stagingRice":"cadenza","declared":"sonata","rices":[{"name":"cadenza","ok":true},{"name":"sonata","ok":true}]}}\n' ;;
"draft save")
  printf '{"status":"ok","message":"saved","data":{"name":"draft-3"}}\n' ;;
"draft drop")
  if [ -f "$here/refuse-drop" ]; then
printf '{"status":"error","message":"cannot drop `%s` — it is the live draft","data":{"reason":"draft-is-live"}}\n' "$4"; exit 1
  fi
  printf '{"status":"ok","message":"dropped %s","data":{}}\n' "$4" ;;
"draft list")
  if [ -f "$here/refuse-list" ]; then
printf '{"status":"error","message":"no rice staged","data":{"reason":"x"}}\n'; exit 1
  fi
  printf '{"status":"ok","message":"2 draft(s)","data":{"drafts":[{"rice":"%s","name":"draft-1","savedAt":"2026-01-01T00:00:00Z","current":false},{"rice":"%s","name":"draft-2","savedAt":null,"current":true}],"scope":"%s"}}\n' "$4" "$4" "$4" ;;
"declare "*)
  if [ -f "$here/refuse-declare" ]; then
printf '{"status":"error","message":"`%s` is not in the rices box builds from","data":{"reason":"not-built-in","refusal":{"kind":"refused","what":"`%s` is not in the rices box builds from","why":"this build carries sonata","fix":{"set":"x"}}}}\n' "$3" "$3"; exit 1
  fi
  printf '{"status":"ok","message":"declared `%s`; takes effect on the next rebuild.","data":{"name":"%s","host":"box","hostConfig":"/h/default.nix","previous":"sonata","changed":true,"stagedDiffers":true}}\n' "$3" "$3" ;;
"mode draft")
  if [ -f "$here/refuse-draft" ]; then
printf '{"status":"error","message":"no rice is currently staged","data":{"reason":"no-resolvable-rice"}}\n'; exit 1
  fi
  printf '{"status":"ok","message":"routed %s","data":{}}\n' "$4" ;;
*)
  printf '{"status":"ok","message":"staged","data":{}}\n' ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(root.join("stage")).unwrap();
    std::fs::write(root.join("stage").join("mode.json"), marker).unwrap();
    let env = aoide_test_support::EnvSaver::capture(&["STRILLIX_BIN", "AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_HOST_CONFIG"]);
    std::env::set_var("STRILLIX_BIN", &stub);
    aoide_test_support::pin_stage(root.join("stage"));
    (env, root)
}

#[cfg(unix)]
pub(crate) fn stub_argv_log(root: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(root.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

// -- workspaceaction, the bar's bind click (W-P5) --

/// A stub `aoide` on `AOIDE_CORE_BIN` (the tier `bin::core_bin` resolves
/// first) that answers with a CLI-shaped `ok` envelope quoting the WHOLE
/// argv it was handed, so a dispatch test asserts both the planned argv
/// and the reply shaping with no daemon and no stage write.
///
/// A stub `aoide` that prints one fixed `--json` envelope and exits `exit`: a core that answers a
/// read the way the CLI does, for the desktop's door calls. Unix only, for the same reason as
/// [`stub_workspace_core`].
#[cfg(unix)]
pub(crate) fn stub_core_answering(tag: &str, envelope: &str, exit: i32) -> (aoide_test_support::EnvSaver, PathBuf) {
    let root = aoide_test_support::unique_tmp(tag);
    std::fs::create_dir_all(&root).unwrap();
    let stub = root.join("aoide");
    std::fs::write(&stub, format!("#!/bin/sh\nprintf '%s\\n' '{envelope}'\nexit {exit}\n")).unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
    std::env::set_var("AOIDE_CORE_BIN", &stub);
    (env, root)
}

/// GATED on Unix with its reason (its callers are gated with it): the stub
/// is a `#!/bin/sh` SCRIPT made executable by a mode, and `CreateProcess`
/// understands neither a shebang nor an extension-less name — the same
/// gate the client crate's own shim groups carry.
#[cfg(unix)]
pub(crate) fn stub_workspace_core(tag: &str) -> (aoide_test_support::EnvSaver, PathBuf) {
    let root = aoide_test_support::unique_tmp(tag);
    std::fs::create_dir_all(&root).unwrap();
    let stub = root.join("aoide");
    std::fs::write(
        &stub,
        r#"#!/bin/sh
printf '{"status":"ok","command":"workspace","message":"argv: %s","data":{"workspace":%s}}\n' "$*" "$3"
"#,
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
    std::env::set_var("AOIDE_CORE_BIN", &stub);
    (env, root)
}

/// A stub `aoide` on `AOIDE_CORE_BIN` that records each call the desktop's door makes: its argv on
/// one line, then its stdin payload (empty when there is none). It exits 1 while `<root>/refuse`
/// exists, which stands in for a core that refuses a call or cannot reach aoided. The record is read
/// from `<root>/calls`; the refusal switch is the file `<root>/refuse`.
///
/// GATED on Unix with its callers, as [`stub_workspace_core`] is: the stub is a `#!/bin/sh` script.
#[cfg(unix)]
pub(crate) fn stub_core_door(tag: &str) -> (aoide_test_support::EnvSaver, PathBuf) {
    let root = aoide_test_support::unique_tmp(tag);
    std::fs::create_dir_all(&root).unwrap();
    let stub = root.join("aoide");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf 'argv: %s\\nstdin: %s\\n' \"$*\" \"$(cat)\" >> '{}'\n[ -e '{}' ] && exit 1\nexit 0\n",
            root.join("calls").display(),
            root.join("refuse").display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let env = aoide_test_support::EnvSaver::capture(&["AOIDE_CORE_BIN"]);
    std::env::set_var("AOIDE_CORE_BIN", &stub);
    (env, root)
}
