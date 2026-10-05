//! Failures of the broker and of the admin commands, said as taught refusals
//! (`pkgs/aoide/crates/AGENTS.md`, "Taught errors"): what was refused, why,
//! and the command or setting that fixes it.
//!
//! The client and the broker report a failure as one line of text (the wire's
//! `error` string). [`classify`] reads the markers those producers share, in
//! one place, so a command turns any such line into a [`Refusal`] without each
//! handler sniffing prose of its own; a test per producer keeps the markers
//! and the producers in step.

use crate::{admin, client, home, policy, store};
use aoide_protocol::output::{Fix, Kind, Refusal};
use aoide_protocol::suggest::closest;
use aoide_protocol::Invocation;
use std::path::Path;

/// Flags of this crate's commands that take no value, for re-quoting a line.
const BOOLS: &[&str] = &["force", "show", "popup", "require-totp"];

const BROKER_USER: &str = "aoide-secrets";

/// What a handler knows about the command that failed.
pub struct Where<'a> {
    pub inv: &'a Invocation,
    pub socket: &'a Path,
    /// The secret the command named, when it named one.
    pub secret: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
enum Fail {
    BrokerUser,
    NotRegistered,
    NotAdmitted,
    NoAccess,
    NotRunning,
    EmptyValue,
    Other,
}

fn classify(err: &str) -> Fail {
    if err.contains(home::MUST_RUN_AS_BROKER) {
        Fail::BrokerUser
    } else if err == client::NOT_REGISTERED || err.starts_with(admin::NO_POLICY) {
        Fail::NotRegistered
    } else if err.contains(admin::NOT_ADMITTED) {
        Fail::NotAdmitted
    } else if err.contains(client::NO_ACCESS) {
        Fail::NoAccess
    } else if err.contains(client::NOT_RUNNING) {
        Fail::NotRunning
    } else if err == client::EMPTY_VALUE {
        Fail::EmptyValue
    } else {
        Fail::Other
    }
}

fn wrapped(inv: &Invocation) -> bool {
    inv.path == ["secrets", "exec"]
}

fn line(inv: &Invocation) -> String {
    inv.command_line("aoide", BOOLS, wrapped(inv))
}

/// A failure line from the broker or an admin mutation, as a refusal.
pub fn from_broker(w: &Where, err: &str) -> Refusal {
    match classify(err) {
        Fail::BrokerUser => broker_user(w.inv, err),
        Fail::NotRegistered => not_registered(w),
        Fail::NotAdmitted => not_admitted(w, err),
        Fail::NoAccess => no_access(w),
        Fail::EmptyValue => empty_value(w.secret.unwrap_or("?"), false),
        Fail::NotRunning => Refusal::new(
            Kind::Refused,
            format!("the secrets broker is not running at {}", w.socket.display()),
            "nothing is listening on that socket; the broker is stopped, or this host keeps its socket elsewhere",
            Fix::Run("systemctl status aoide-secrets-serve".to_string()),
        ),
        Fail::Other => Refusal::new(
            Kind::Failed,
            err.to_string(),
            "the broker or the store reported it; the line above is its own wording",
            Fix::None("nothing to change in this command; the cause is in the line above"),
        ),
    }
}

/// A value that is empty or only whitespace: refused by `put` at the client
/// (`typed` says a person was at the prompt) and again by the broker's own
/// put gate, so a caller that skips the client cannot store one either.
pub fn empty_value(name: &str, typed: bool) -> Refusal {
    if typed {
        return Refusal::new(
            Kind::Refused,
            "nothing was typed at the prompt",
            "an empty value stored under a secret reads as no secret at all to every check built on it",
            Fix::Run(format!("aoide secrets put {name}")),
        );
    }
    Refusal::new(
        Kind::Refused,
        "nothing arrived on stdin",
        "the command before the pipe printed nothing or failed (e.g. `openssl: command not found`)",
        Fix::Run(format!("head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \\n' | aoide secrets put {name} --force")),
    )
}

/// An admin command run by someone other than the broker user.
pub fn broker_user(inv: &Invocation, err: &str) -> Refusal {
    let what = err.split(" Run: ").next().unwrap_or(err).trim_end_matches('.').to_string();
    Refusal::new(
        Kind::Refused,
        what,
        "an admin command writes the broker's policy file, and only the broker user may: a write by anyone else, root included, re-owns it and bricks the broker",
        Fix::Run(format!("sudo -u {BROKER_USER} {}", line(inv))),
    )
}

fn no_access(w: &Where) -> Refusal {
    let sg = line(w.inv).replace('\'', "'\\''");
    Refusal::new(
        Kind::Refused,
        format!("this session may not open the secrets broker's socket at {}", w.socket.display()),
        format!(
            "it is not in the `aoide-secrets-access` group; group membership is login-scoped, so a fresh login picks it up, or `sg aoide-secrets-access -c '{sg}'` borrows it for one command"
        ),
        Fix::Set(
            "add your user to `aoide.secrets.members` in this host's config, rebuild, then log in again".to_string(),
        ),
    )
}

fn registered(socket: &Path) -> Vec<String> {
    match client::status(socket) {
        Ok(report) => report.secrets.into_iter().map(|s| s.name).collect(),
        Err(_) => store::load_policies(&home::secrets_home())
            .map(|ps| ps.into_iter().map(|p| p.name).collect())
            .unwrap_or_default(),
    }
}

fn renamed(inv: &Invocation, old: &str, new: &str) -> Invocation {
    let mut out = inv.clone();
    for arg in out.args.iter_mut().filter(|a| a.as_str() == old) {
        *arg = new.to_string();
    }
    for value in out.flags.values_mut() {
        if value.as_str() == old || value.starts_with(&format!("{old}:")) {
            *value = value.replacen(old, new, 1);
        }
    }
    out
}

fn not_registered(w: &Where) -> Refusal {
    let name = w.secret.unwrap_or("?");
    let names = registered(w.socket);
    let what = format!("secret `{name}` is not registered");
    if let Some(near) = closest(name, names.iter().map(String::as_str), 1).first() {
        return Refusal::new(
            Kind::Refused,
            what,
            format!("the broker holds no policy by that name; did you mean `{near}`?"),
            Fix::Run(line(&renamed(w.inv, name, near))),
        );
    }
    let why = if names.is_empty() {
        "the broker holds no policy by that name, and none is registered yet".to_string()
    } else {
        format!("the broker holds no policy by that name; the registered secrets are {}", names.join(", "))
    };
    Refusal::new(Kind::Refused, what, why, Fix::Run(format!("aoide secrets add {name}")))
}

fn not_admitted(w: &Where, err: &str) -> Refusal {
    let name = w.secret.unwrap_or("?");
    let consumer = w.inv.args.get(if w.inv.path[1] == "automate" { 2 } else { 1 }).map(String::as_str).unwrap_or("?");
    let admits = err
        .split_once("(it admits ")
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(list, _)| format!(" (it admits {list})"))
        .unwrap_or_default();
    Refusal::new(
        Kind::Refused,
        format!("secret `{name}` does not admit consumer `{consumer}`{admits}"),
        "automation only skips the TOTP code for a consumer the secret already admits, and the broker checks the admitted list first, so this grant could never open anything (an empty list admits every consumer)",
        Fix::Run(format!("aoide secrets grant {name} {consumer}")),
    )
}

/// A secret or consumer name the policy store would never accept.
pub fn invalid_name(inv: &Invocation, kind: &str, name: &str) -> Refusal {
    let mut fixed: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '-' })
        .collect();
    while fixed.contains("--") {
        fixed = fixed.replace("--", "-");
    }
    let fixed = fixed.trim_matches('-').to_string();
    let fix = if policy::valid_secret_name(&fixed) {
        Fix::Run(line(&renamed(inv, name, &fixed)))
    } else {
        Fix::Set("pick a name of lowercase letters, digits and single hyphens, such as `db-prod`".to_string())
    };
    Refusal::new(
        Kind::Usage,
        format!("`{name}` is not a valid {kind} name"),
        "names are lowercase letters, digits and single hyphens, and begin and end with a letter or digit",
        fix,
    )
}

/// A command that only a person at a terminal may run, reached over another door.
pub fn cli_only(inv: &Invocation, why: &str) -> Refusal {
    Refusal::new(
        Kind::Usage,
        format!("`aoide {}` is CLI-only", inv.path.join(" ")),
        why.to_string(),
        Fix::Run(line(inv)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::output::Outcome;
    use aoide_protocol::Door;
    use std::collections::BTreeMap;

    fn inv(path: &[&str], args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
            door: Door::Cli,
        }
    }

    fn fix_of(r: Refusal) -> String {
        match r.fix {
            Fix::Run(s) | Fix::Set(s) | Fix::Wait(s) => s,
            Fix::None(s) => s.to_string(),
        }
    }

    #[test]
    fn every_producer_of_a_classified_failure_reads_back_as_its_kind() {
        let owner = crate::peercred::PeerUser::Uid(0);
        let me = crate::peercred::PeerUser::Uid(1000);
        let wrong_owner = home::admin_identity_error(&me, &owner, Path::new("/h"), "add").unwrap();
        let missing_home = home::admin_identity_error_for_missing_home(&owner, Path::new("/h"), "add").unwrap();
        for msg in [wrong_owner, missing_home] {
            assert_eq!(classify(&msg), Fail::BrokerUser, "{msg}");
        }
        assert_eq!(classify(client::NOT_REGISTERED), Fail::NotRegistered);
        assert_eq!(classify(&format!("{} `x`", admin::NO_POLICY)), Fail::NotRegistered);
        let denied = client::describe_connect_error(
            Path::new("/s"),
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            "aoide secrets status",
        );
        assert_eq!(classify(&denied), Fail::NoAccess);
        let down = client::describe_connect_error(
            Path::new("/s"),
            &std::io::Error::from(std::io::ErrorKind::NotFound),
            "aoide secrets status",
        );
        assert_eq!(classify(&down), Fail::NotRunning);
        assert_eq!(classify(client::EMPTY_VALUE), Fail::EmptyValue);
        assert_eq!(classify("something else"), Fail::Other);
    }

    #[test]
    fn the_broker_user_fix_repeats_the_whole_command_the_user_typed() {
        let i = inv(&["secrets", "automate"], &["sudo-pass", "grant", "orchestrator"], &[]);
        let r = broker_user(&i, "secrets automate must run as the broker user (uid 0) — this process is running as uid 1000");
        assert_eq!(fix_of(r), "sudo -u aoide-secrets aoide secrets automate sudo-pass grant orchestrator");
    }

    #[test]
    fn an_old_broker_message_loses_its_ellipsis_run_line() {
        let i = inv(&["secrets", "add"], &["x"], &[("key", "x")]);
        let r = broker_user(&i, "secrets add must run as the broker user (uid 0). Run: sudo -u aoide-secrets aoide secrets add ...");
        assert!(!r.what.contains("..."), "{}", r.what);
        assert_eq!(fix_of(r), "sudo -u aoide-secrets aoide secrets add x --key x");
    }

    #[test]
    fn an_invalid_name_is_fixed_by_its_normal_form() {
        let i = inv(&["secrets", "add"], &["Bad__Name"], &[("key", "k")]);
        let r = invalid_name(&i, "secret", "Bad__Name");
        assert_eq!(r.kind, Kind::Usage);
        assert_eq!(fix_of(r), "aoide secrets add bad-name --key k");
        let hopeless = invalid_name(&i, "secret", "___");
        assert!(matches!(hopeless.fix, Fix::Set(_)));
    }

    #[test]
    fn a_refusal_renders_with_a_fix_line_and_a_json_refusal() {
        let i = inv(&["secrets", "automate"], &["x", "grant", "nobody"], &[]);
        let w = Where { inv: &i, socket: Path::new("/nonexistent/s.sock"), secret: Some("x") };
        let out: Outcome = from_broker(&w, "secret `x` does not admit consumer `nobody` (it admits m, verba) — automation").into_outcome("secrets.automate");
        let text = out.render(false).0;
        assert!(text.contains("does not admit consumer `nobody` (it admits m, verba)"), "{text}");
        assert!(text.contains("fix: aoide secrets grant x nobody"), "{text}");
    }
}
