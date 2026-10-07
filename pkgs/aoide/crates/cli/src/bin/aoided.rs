//! The `aoided` binary — the resident orchestrator daemon (P-D2/P-D4,
//! `docs/architecture/AOIDED.md`).
//!
//! Owns the single policy surface: audit log, user rebuild gate, neutral event
//! stream with default-deny-per-class subscriptions (entities/aoided). Binds
//! its own control socket (`ping`/`subscribe`/`dispatch` — the fourth door
//! onto this binary's own registry) and runs
//! forever — this is what the systemd unit execs (`modules/nucleus/
//! aoided.nix`, `Type=simple` + `Restart=on-failure` as of this phase).
//! `dispatch::registry()`/`dispatch::dispatch` are injected here — the SAME
//! DI seam `mcp serve --stdio`/`a2a serve` close at their own launch sites
//! (`lib.rs`'s `run_cli`) — because `aoide-server` sits BELOW this crate and
//! cannot reach the fully-assembled registry itself
//! (`aoide_server::daemon`'s own module doc).

use aoide::daemon;
use aoide::dispatch;
use aoide_protocol::output::{Fix, Kind, Outcome};
use aoide_protocol::registry::AOIDE_VERSION;
use aoide_protocol::{door, suggest};
use std::path::PathBuf;

const BIN: &str = "aoided";

struct Flag {
    name: &'static str,
    value: Option<&'static str>,
    help: &'static str,
}

const AUDIT_LOG: Flag = Flag { name: "--audit-log", value: Some("<path>"), help: "write the audit log there instead of $AOIDE_ROOT/log" };
const VERSION: Flag = Flag { name: "--version", value: None, help: "print the version and exit" };
const HELP: Flag = Flag { name: "--help", value: None, help: "print this page and exit" };
const FLAGS: [Flag; 3] = [AUDIT_LOG, VERSION, HELP];

impl Flag {
    fn spelled(&self) -> String {
        self.value.map_or(self.name.to_string(), |v| format!("{} {v}", self.name))
    }
}

fn usage() -> String {
    let forms: Vec<String> = FLAGS.iter().map(Flag::spelled).collect();
    let width = forms.iter().map(String::len).max().unwrap_or(0);
    let rows: Vec<String> = FLAGS.iter().zip(&forms).map(|(f, form)| format!("  {form:<width$}  {}", f.help)).collect();
    format!(
        "aoided — the resident Aoide daemon: one policy surface, one gate, one audit log\nusage: {BIN} [flag]\n\n{}\n\n{BIN} takes no other arguments; it runs in the foreground until stopped.",
        rows.join("\n")
    )
}

#[derive(Debug, PartialEq)]
enum Launch {
    Help,
    Version,
    Run { audit_log: Option<PathBuf> },
}

fn parse(argv: &[String]) -> Result<Launch, Outcome> {
    let (mut help, mut version, mut audit_log) = (false, false, None);
    let mut args = argv.iter();
    while let Some(arg) = args.next() {
        let Some(flag) = FLAGS.iter().find(|f| f.name == arg) else {
            let near = suggest::closest(arg, FLAGS.iter().map(|f| f.name), 1);
            let fix = match (arg.strip_prefix(&format!("{}=", AUDIT_LOG.name)), near.first()) {
                (Some(path), _) => Fix::Run(format!("aoided {} {path}", AUDIT_LOG.name)),
                (None, Some(near)) => Fix::Run(format!("aoided {near}")),
                (None, None) => Fix::Run(format!("aoided {}", HELP.name)),
            };
            let known: Vec<String> = FLAGS.iter().map(Flag::spelled).collect();
            return Err(Outcome::refuse(
                "aoided",
                Kind::Usage,
                format!("`{arg}` is not an aoided argument"),
                format!("aoided takes only {}, and starts nothing on anything else", known.join(", ")),
                fix,
            ));
        };
        if flag.name == HELP.name {
            help = true;
        } else if flag.name == VERSION.name {
            version = true;
        } else {
            match args.next().filter(|v| !v.starts_with("--")) {
                Some(path) => audit_log = Some(PathBuf::from(path)),
                None => {
                    return Err(Outcome::refuse(
                        "aoided",
                        Kind::Usage,
                        format!("`{}` needs a path", flag.name),
                        "it names the file the daemon appends its audit log to",
                        Fix::Run(format!("aoided {}", flag.spelled())),
                    ))
                }
            }
        }
    }
    Ok(if help {
        Launch::Help
    } else if version {
        Launch::Version
    } else {
        Launch::Run { audit_log }
    })
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let audit_log = match parse(&argv) {
        Ok(Launch::Help) => std::process::exit(door::say(&usage()).unwrap_or(0)),
        Ok(Launch::Version) => std::process::exit(door::say(&format!("aoided {AOIDE_VERSION}")).unwrap_or(0)),
        Ok(Launch::Run { audit_log }) => audit_log,
        Err(refusal) => std::process::exit(door::emit(&refusal, false)),
    };

    // One-shot, idempotent `~/Aoide` → `$AOIDE_ROOT` migration (L-C2, task
    // #107) — see `aoide_storage::fs::root`'s own doc for why this runs
    // here, explicitly, rather than hanging off a path getter. It runs only
    // once the argv has proven this is a launch: a probe touches nothing.
    aoide_storage::fs::migrate_root_once();

    let log = audit_log.unwrap_or_else(daemon::default_audit_log);
    let socket = daemon::socket_path();
    let events = daemon::events_path(&socket);

    if let Err(e) = daemon::run_loop(socket, events, log, dispatch::registry(), dispatch::dispatch) {
        eprintln!("aoided: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_launches_on_the_ambient_log() {
        assert_eq!(parse(&[]).unwrap(), Launch::Run { audit_log: None });
    }

    #[test]
    fn audit_log_takes_its_path() {
        assert_eq!(parse(&argv(&["--audit-log", "/x/log"])).unwrap(), Launch::Run { audit_log: Some("/x/log".into()) });
    }

    #[test]
    fn audit_log_without_a_path_is_usage() {
        for a in [argv(&["--audit-log"]), argv(&["--audit-log", "--version"])] {
            assert_eq!(parse(&a).unwrap_err().render(false).1, 2);
        }
    }

    #[test]
    fn a_typo_is_refused_with_the_near_flag() {
        let (text, code) = parse(&argv(&["--verison"])).unwrap_err().render(false);
        assert_eq!(code, 2);
        assert!(text.contains("aoided --version"), "{text}");
    }

    #[test]
    fn the_equals_form_is_taught_the_spaced_one() {
        let (text, code) = parse(&argv(&["--audit-log=/x"])).unwrap_err().render(false);
        assert_eq!(code, 2);
        assert!(text.contains("aoided --audit-log /x"), "{text}");
    }

    #[test]
    fn a_far_argument_points_at_help() {
        let (text, code) = parse(&argv(&["start"])).unwrap_err().render(false);
        assert_eq!(code, 2);
        assert!(text.contains("aoided --help"), "{text}");
    }
}
