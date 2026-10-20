//! The `component` verb: one `strillix component <slot> <verb> [<arg>]` call,
//! spawned through the compositor's exec dispatch.

/// A component slot name: lowercase start, then lowercase, digits or `-`.
pub(crate) fn component_slot(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A component verb: an ASCII letter start, then ASCII letters or digits
/// (`openTab` is camelCase by contract).
pub(crate) fn component_verb(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric())
}

/// A component verb's one argument: 1 to 64 characters of ASCII letters,
/// digits, `.`, `_` or `-`, never a leading `-` (a flag to the provider).
pub(crate) fn component_arg(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && !s.starts_with('-')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The program and argv that run `strillix component <slot> <verb> [arg]`. It goes
/// through the compositor (`hyprctl dispatch exec`), as `PowerAction::Lock`
/// does, and never as this unit's child: NoNewPrivileges is inherited, and a
/// provider's action may reach a setuid helper. The charsets above make the one
/// spelled line safe for the compositor's shell.
pub(crate) fn component_command(slot: &str, verb: &str, arg: Option<&str>) -> (&'static str, Vec<String>) {
    let mut line = format!("strillix component {slot} {verb}");
    if let Some(arg) = arg {
        line.push(' ');
        line.push_str(arg);
    }
    ("hyprctl", vec!["dispatch".to_string(), "exec".to_string(), line])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{parse_command, BridgeCommand};

    #[test]
    fn parse_command_accepts_a_component_call_with_and_without_an_arg() {
        assert_eq!(
            parse_command(r#"{"cmd":"component","slot":"powermenu","verb":"show","arg":"lock"}"#),
            Some(BridgeCommand::Component {
                slot: "powermenu".into(),
                verb: "show".into(),
                arg: Some("lock".into()),
            })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"component","slot":"dock","verb":"openTab"}"#),
            Some(BridgeCommand::Component { slot: "dock".into(), verb: "openTab".into(), arg: None })
        );
        assert_eq!(
            parse_command(r#"{"cmd":"component","slot":"calendar","verb":"toggle","arg":"DP-1"}"#),
            Some(BridgeCommand::Component {
                slot: "calendar".into(),
                verb: "toggle".into(),
                arg: Some("DP-1".into()),
            })
        );
    }

    #[test]
    fn parse_command_rejects_a_component_name_outside_its_charset() {
        let refused = [
            r#"{"cmd":"component","slot":"pow;rm -rf ~","verb":"toggle"}"#,
            r#"{"cmd":"component","slot":"Powermenu","verb":"toggle"}"#,
            r#"{"cmd":"component","slot":"","verb":"toggle"}"#,
            r#"{"cmd":"component","slot":"dock","verb":"to ggle"}"#,
            r#"{"cmd":"component","slot":"dock","verb":"9toggle"}"#,
            r#"{"cmd":"component","slot":"dock","verb":"toggle","arg":"DP-1 && x"}"#,
            r#"{"cmd":"component","slot":"dock","verb":"toggle","arg":"--evil"}"#,
            r#"{"cmd":"component","slot":"dock","verb":"toggle","arg":""}"#,
            r#"{"cmd":"component","slot":"dock","verb":"toggle","arg":7}"#,
            r#"{"cmd":"component","verb":"toggle"}"#,
            r#"{"cmd":"component","slot":"dock"}"#,
        ];
        for line in refused {
            assert_eq!(parse_command(line), None, "must refuse {line}");
        }
        let too_long = format!(r#"{{"cmd":"component","slot":"dock","verb":"toggle","arg":"{}"}}"#, "a".repeat(65));
        assert_eq!(parse_command(&too_long), None, "an arg over 64 characters is refused");
    }

    #[test]
    fn a_component_call_is_spelled_as_one_line_through_the_compositor() {
        assert_eq!(
            component_command("powermenu", "show", Some("lock")),
            ("hyprctl", vec!["dispatch".into(), "exec".into(), "strillix component powermenu show lock".into()])
        );
        assert_eq!(
            component_command("dock", "toggle", None),
            ("hyprctl", vec!["dispatch".into(), "exec".into(), "strillix component dock toggle".into()])
        );
    }
}
