//! `aoide secrets` through the REAL binary against a temp broker: the refusals
//! a person actually reads (what/why/fix on stderr, `--json` carrying the same),
//! the empty-stdin refusal of `put`, and the `--key` default.
//!
//! Every child runs under an ISOLATED AOIDE_ROOT and its own broker home and
//! socket, so neither the real `~/.aoide` nor the live broker is touched.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Rig {
    root: PathBuf,
    home: PathBuf,
    socket: PathBuf,
}

struct Out {
    stdout: String,
    stderr: String,
    code: i32,
}

impl Rig {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("aoide-secrets-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let socket = aoide_test_support::short_tmp(&format!("secrets-cli-{tag}-{}", std::process::id())).join("b.sock");
        let rig = Rig { root, home, socket };
        let (h, s) = (rig.home.clone(), rig.socket.clone());
        std::thread::spawn(move || {
            let _ = aoide_secrets::broker::serve(&h, &s);
        });
        for _ in 0..100 {
            if std::os::unix::net::UnixStream::connect(&rig.socket).is_ok() {
                return rig;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the broker did not bind {}", rig.socket.display());
    }

    fn aoide(&self, args: &[&str], stdin: Option<&str>) -> Out {
        self.aoide_in(&self.home, &self.socket, args, stdin)
    }

    fn aoide_in(&self, home: &std::path::Path, socket: &std::path::Path, args: &[&str], stdin: Option<&str>) -> Out {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aoide"))
            .args(args)
            .env("NO_COLOR", "1")
            .env("AOIDE_ROOT", &self.root)
            .env("AOIDE_STAGE_DIR", self.root.join("stage"))
            .env("AOIDE_STATE_DIR", self.root.join("state"))
            .env("AOIDE_AUDIT_LOG", self.root.join("log"))
            .env("AOIDE_SECRETS_HOME", home)
            .env("AOIDE_SECRETS_SOCKET", socket)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the aoide binary runs");
        let mut pipe = child.stdin.take().unwrap();
        if let Some(text) = stdin {
            pipe.write_all(text.as_bytes()).unwrap();
        }
        drop(pipe);
        let out = child.wait_with_output().unwrap();
        Out {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        if let Some(dir) = self.socket.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[test]
fn add_for_age_defaults_the_key_to_the_secrets_name() {
    let rig = Rig::new("key");
    let out = rig.aoide(&["secrets", "add", "db-prod"], None);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let policy = std::fs::read_to_string(rig.home.join("policy.json")).unwrap();
    assert!(policy.contains("\"key\": \"db-prod\"") || policy.contains("\"key\":\"db-prod\""), "{policy}");
}

#[test]
fn put_refuses_empty_or_blank_stdin_with_what_why_and_fix_and_stores_nothing() {
    let rig = Rig::new("put");
    assert_eq!(rig.aoide(&["secrets", "add", "x"], None).code, 0);
    for blank in ["", "\n", " \n\t\n"] {
        let out = rig.aoide(&["secrets", "put", "x"], Some(blank));
        assert_eq!(out.code, 1, "{blank:?}: {}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains("nothing arrived on stdin"), "{}", out.stderr);
        assert!(out.stderr.contains("why: the command before the pipe printed nothing or failed"), "{}", out.stderr);
        assert!(
            out.stderr.contains("fix: head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \\n' | aoide secrets put x --force"),
            "{}",
            out.stderr
        );
    }
    assert!(!rig.home.join("values").join("x.age").exists(), "an empty value must not be stored");
}

#[test]
fn exec_of_an_unregistered_secret_names_it_suggests_a_near_one_and_honours_json() {
    let rig = Rig::new("exec");
    assert_eq!(rig.aoide(&["secrets", "add", "db-prod"], None).code, 0);
    let out = rig.aoide(&["secrets", "exec", "--as", "a", "--secret", "db-prd", "--", "true"], None);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("secret `db-prd` is not registered"), "{}", out.stderr);
    assert!(out.stderr.contains("did you mean `db-prod`?"), "{}", out.stderr);
    assert!(out.stderr.contains("fix: aoide secrets exec --as a --secret db-prod -- true"), "{}", out.stderr);
    assert!(!out.stderr.contains("secrets exec: secrets exec:"), "{}", out.stderr);

    let far = rig.aoide(&["secrets", "exec", "--as", "a", "--secret", "nope", "--", "true"], None);
    assert!(far.stderr.contains("the registered secrets are db-prod"), "{}", far.stderr);
    assert!(far.stderr.contains("fix: aoide secrets add nope"), "{}", far.stderr);

    let json = rig.aoide(&["secrets", "exec", "--as", "a", "--secret", "nope", "--json", "--", "true"], None);
    assert_eq!(json.code, 1);
    let v: serde_json::Value = serde_json::from_str(json.stderr.trim()).unwrap_or_else(|e| panic!("{e}: {}", json.stderr));
    assert_eq!(v["data"]["refusal"]["what"], "secret `nope` is not registered");
    assert_eq!(v["data"]["refusal"]["fix"]["run"], "aoide secrets add nope");
}

#[test]
fn exec_without_its_required_flags_is_a_usage_refusal_not_a_prefixed_line() {
    let rig = Rig::new("exec-usage");
    let out = rig.aoide(&["secrets", "exec", "--secret", "x", "--", "true"], None);
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(out.stderr.contains("needs --as <consumer>"), "{}", out.stderr);
    assert!(!out.stderr.contains("usage:"), "{}", out.stderr);
}

#[test]
fn an_automation_grant_the_secret_does_not_admit_is_refused_with_the_grant_fix() {
    let rig = Rig::new("automate");
    assert_eq!(rig.aoide(&["secrets", "add", "x", "--consumers", "m"], None).code, 0);
    let out = rig.aoide(&["secrets", "automate", "x", "grant", "nobody"], None);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("secret `x` does not admit consumer `nobody` (it admits m)"), "{}", out.stderr);
    assert!(out.stderr.contains("fix: aoide secrets grant x nobody"), "{}", out.stderr);

    assert_eq!(rig.aoide(&["secrets", "add", "open"], None).code, 0);
    let open = rig.aoide(&["secrets", "automate", "open", "grant", "anyone"], None);
    assert_eq!(open.code, 0, "an empty consumers list admits everyone: {}", open.stderr);
}

#[test]
fn a_state_outside_on_off_is_refused_before_anything_runs() {
    let rig = Rig::new("state");
    let out = rig.aoide(&["secrets", "set-totp", "x", "maybe"], None);
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(out.stderr.contains("does not accept `maybe` for <state>"), "{}", out.stderr);
    assert!(out.stderr.contains("the accepted values are on, off"), "{}", out.stderr);
}

#[test]
fn the_broker_user_refusal_prints_the_whole_command_to_paste() {
    use std::os::unix::fs::MetadataExt;
    let rig = Rig::new("broker-user");
    let mine = std::fs::metadata(&rig.root).unwrap().uid();
    // The nix build sandbox has no /usr, and inside it everything may be ours.
    let Some(foreign) = ["/usr", "/etc", "/"]
        .into_iter()
        .map(std::path::Path::new)
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.uid() != mine))
    else {
        return;
    };
    let nowhere = rig.root.join("no-broker").join("s.sock");
    let out = rig.aoide_in(foreign, &nowhere, &["secrets", "automate", "sudo-pass", "grant", "orchestrator"], None);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("must run as the broker user"), "{}", out.stderr);
    assert!(
        out.stderr.contains("fix: sudo -u aoide-secrets aoide secrets automate sudo-pass grant orchestrator"),
        "{}",
        out.stderr
    );
    assert!(!out.stderr.contains("..."), "{}", out.stderr);
}
