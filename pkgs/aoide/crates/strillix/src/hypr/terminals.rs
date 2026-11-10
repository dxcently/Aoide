//! The terminal list: every terminal window the compositor shows, written whole to
//! `stage/terminals.json` under the rice runtime root whenever it changes ([`TerminalList`]). Core
//! is not told: a terminal is a session only once something tracks it.

use super::door::Latch;
use aoide_conduct::graph::{proc_cwd, TermWindow};
use aoide_storage::stage::write_stage;
use serde_json::{json, Value};

/// Extract a [`TermWindow`] from one `hyprctl clients -j` client object, reading
/// its owning process's cwd from `/proc/<pid>/cwd`. `None` for a client with no
/// usable `address`. The one client read the terminal list and the sweep share.
pub(crate) fn term_window_from_client(c: &Value) -> Option<TermWindow> {
    let address = c.get("address").and_then(Value::as_str)?.trim().to_string();
    if address.is_empty() {
        return None;
    }
    let class = c
        .get("class")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let title = c
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let workspace = c
        .get("workspace")
        .and_then(|w| w.get("id"))
        .and_then(Value::as_i64);
    // Checked, not `as i32`: hyprctl's JSON pid is an i64, and a value outside
    // i32's range would otherwise silently wrap into an arbitrary in-range
    // pid before it's used both for `/proc/<pid>/cwd` below and, later, as the
    // window's owner pid in the sweep. `try_from` simply
    // drops an out-of-range value (`None`) rather than wrapping it into a
    // bogus one.
    let pid = c
        .get("pid")
        .and_then(Value::as_i64)
        .and_then(|p| i32::try_from(p).ok())
        .filter(|p| *p > 0);
    // `mapped` defaults to true when absent — a client with no `mapped` field is
    // treated as a shown window rather than silently dropped.
    let mapped = c.get("mapped").and_then(Value::as_bool).unwrap_or(true);
    let cwd = pid.and_then(proc_cwd).unwrap_or_default();
    Some(TermWindow {
        address,
        class,
        title,
        workspace,
        pid,
        mapped,
        cwd,
    })
}

/// The window classes the terminal list counts, compared trimmed and lowercased.
const TERMINAL_CLASSES: &[&str] = &[
    "kitty",
    "foot",
    "footclient",
    "alacritty",
    "wezterm",
    "org.wezfurlong.wezterm",
    "ghostty",
    "com.mitchellh.ghostty",
    "xterm",
    "uxterm",
    "konsole",
    "urxvt",
    "rxvt",
    "termite",
    "tilix",
    "contour",
    "rio",
    "st",
    "kgx",
    "org.gnome.console",
    "blackbox",
    "terminator",
    "sakura",
    "wave",
    "xfce4-terminal",
    "gnome-terminal",
    "qterminal",
    "lxterminal",
    "deepin-terminal",
];

/// Whether the list counts a window: a shown one, of a terminal class.
fn is_terminal(w: &TermWindow) -> bool {
    w.mapped && TERMINAL_CLASSES.contains(&w.class.trim().to_ascii_lowercase().as_str())
}

/// The list this tick: each terminal window, in the clients' order.
fn terminal_list(clients: &[Value]) -> Value {
    let terminals: Vec<Value> = clients
        .iter()
        .filter_map(term_window_from_client)
        .filter(is_terminal)
        .map(|w| {
            json!({
                "address": w.address,
                "class": w.class,
                "title": w.title,
                "workspace": w.workspace,
                "pid": w.pid,
                "cwd": w.cwd,
            })
        })
        .collect();
    json!({ "schemaVersion": "0", "terminals": terminals })
}

/// Writes the terminal list to `stage/terminals.json` under the rice runtime root when it differs
/// from the list last written. A write that fails is logged once and tried again on the next
/// event or tick. [`TerminalList::forget`], called on each reconnect, makes the next settle write
/// the list again.
#[derive(Default)]
pub(crate) struct TerminalList {
    written: Option<Value>,
}

impl TerminalList {
    pub(crate) fn forget(&mut self) {
        self.written = None;
    }

    pub(crate) fn settle(&mut self, clients: &[Value], latch: &mut Latch) {
        let list = terminal_list(clients);
        if self.written.as_ref() == Some(&list) {
            return;
        }
        match write_stage(&strillix_rice::paths::stage_dir().join("terminals.json"), &list) {
            Ok(()) => {
                latch.recovered("terminals-write");
                self.written = Some(list);
            }
            Err(e) => latch.problem(
                "terminals-write",
                &format!("[strillix/shellbridge] the terminal list was not written: {e}"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list holds each shown window of a terminal class, in the clients' order, with `null` for
    /// a workspace or pid the compositor gives none of and `""` for a cwd it cannot read. A hidden
    /// terminal and a window of another class are left out.
    #[test]
    fn the_list_holds_the_shown_terminal_windows() {
        let clients = [
            json!({ "address": "0xAA", "class": "kitty", "title": "t", "workspace": { "id": 2 } }),
            json!({ "address": "0xBB", "class": "Foot" }),
            json!({ "address": "0xCC", "class": "firefox" }),
            json!({ "address": "0xDD", "class": "kitty", "mapped": false }),
        ];
        let list = terminal_list(&clients);
        assert_eq!(list["schemaVersion"], "0");
        let rows = list["terminals"].as_array().unwrap();
        let addresses: Vec<&str> = rows.iter().map(|r| r["address"].as_str().unwrap()).collect();
        assert_eq!(addresses, ["0xAA", "0xBB"]);
        assert_eq!(rows[0]["workspace"], 2);
        assert_eq!(
            rows[1],
            json!({ "address": "0xBB", "class": "Foot", "title": "", "workspace": null, "pid": null, "cwd": "" })
        );
    }
}
