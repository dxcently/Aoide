//! The `power` verb: the powermenu's six system actions, each spawned detached and
//! never waited for. Several of them end the very process that would wait: logout
//! tears the session down, and suspend stops the clock.

/// The six system actions the powermenu can request. A closed set — an unknown
/// action string parses to `None` at the wire, never to a dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Lock,
    Logout,
    Suspend,
    Hibernate,
    Reboot,
    Shutdown,
}

impl PowerAction {
    /// Parse the wire's `action` string. Case-sensitive lowercase by contract
    /// (ShellBridge.qml sends exactly these), anything else is `None`.
    pub(crate) fn from_wire(s: &str) -> Option<Self> {
        match s {
            "lock" => Some(Self::Lock),
            "logout" => Some(Self::Logout),
            "suspend" => Some(Self::Suspend),
            "hibernate" => Some(Self::Hibernate),
            "reboot" => Some(Self::Reboot),
            "shutdown" => Some(Self::Shutdown),
            _ => None,
        }
    }

    /// The program + args this action spawns. `lock` asks the compositor to
    /// run `strillix lock` (the one lock entry: the shell's lock host for a rice
    /// with a face, else hyprlock) rather than spawning it here: this unit
    /// runs with NoNewPrivileges, which a child inherits, and hyprlock's PAM
    /// check needs the setuid `unix_chkpwd`, so a locker spawned here refuses
    /// every password. `logout` exits the compositor; the rest are systemd
    /// commands.
    pub(crate) fn command(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Lock => ("hyprctl", &["dispatch", "exec", "strillix lock"]),
            Self::Logout => ("hyprctl", &["dispatch", "exit"]),
            Self::Suspend => ("systemctl", &["suspend"]),
            Self::Hibernate => ("systemctl", &["hibernate"]),
            Self::Reboot => ("systemctl", &["reboot"]),
            Self::Shutdown => ("systemctl", &["poweroff"]),
        }
    }

    /// The wire name back, for audit lines.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Logout => "logout",
            Self::Suspend => "suspend",
            Self::Hibernate => "hibernate",
            Self::Reboot => "reboot",
            Self::Shutdown => "shutdown",
        }
    }
}

/// Dispatch ONE power action: spawn the mapped command and return. NEVER waits
/// for exit — several of these actions kill or freeze the very process that
/// would be waiting (logout tears the session down, suspend stops the clock) —
/// a detached reaper thread collects the child's status so it never lingers as
/// a zombie. A spawn failure is an `Err` for the caller to log; nothing here
/// can take down the accept loop.
pub(crate) fn dispatch_power(action: PowerAction) -> std::io::Result<()> {
    let (prog, args) = action.command();
    spawn_detached(prog, args)
}

/// Spawn `prog args` and return at once: a detached reaper collects the
/// child's status, so it never lingers as a zombie. Never waits — the caller
/// may be an action that kills or freezes what would be waiting.
pub(crate) fn spawn_detached<S: AsRef<str>>(prog: &str, args: &[S]) -> std::io::Result<()> {
    let mut child = std::process::Command::new(prog).args(args.iter().map(|a| a.as_ref())).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::testutil::env_lock;
    use crate::bridge::{parse_command, BridgeCommand};

    #[test]
    fn parse_command_accepts_every_valid_power_action() {
        let cases = [
            ("lock", PowerAction::Lock),
            ("logout", PowerAction::Logout),
            ("suspend", PowerAction::Suspend),
            ("hibernate", PowerAction::Hibernate),
            ("reboot", PowerAction::Reboot),
            ("shutdown", PowerAction::Shutdown),
        ];
        for (wire, want) in cases {
            assert_eq!(
                parse_command(&format!(r#"{{"cmd":"power","action":"{wire}"}}"#)),
                Some(BridgeCommand::Power { action: want }),
                "power action {wire} must parse"
            );
        }
        // Whitespace around the action is trimmed (wire lines arrive
        // newline-terminated), same tolerance as focuswindow's address.
        assert_eq!(
            parse_command("  {\"cmd\":\"power\",\"action\":\" lock \"}\n"),
            Some(BridgeCommand::Power {
                action: PowerAction::Lock
            })
        );
    }

    #[test]
    fn lock_is_started_by_the_compositor_never_as_this_units_child() {
        let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (prog, args) = PowerAction::Lock.command();
        assert_eq!((prog, args), ("hyprctl", &["dispatch", "exec", "strillix lock"][..]));
    }

    #[test]
    fn parse_command_rejects_bad_power_actions() {
        // Unknown action → None (a typo must never reach a dispatch).
        assert_eq!(parse_command(r#"{"cmd":"power","action":"explode"}"#), None);
        // Case matters — the wire contract is lowercase.
        assert_eq!(parse_command(r#"{"cmd":"power","action":"Reboot"}"#), None);
        // Empty / absent / non-string action → None.
        assert_eq!(parse_command(r#"{"cmd":"power","action":""}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"power"}"#), None);
        assert_eq!(parse_command(r#"{"cmd":"power","action":42}"#), None);
    }
}
