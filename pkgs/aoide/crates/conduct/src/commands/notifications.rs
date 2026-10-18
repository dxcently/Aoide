//! The notifications' CLI command: `notifications push`, dunst's `script` hook.
//!
//! One command, and a machine-facing one — a human never types it. dunst runs it
//! per notification with the `DUNST_*` environment set (see
//! `modules/dendrites/dunst.nix`), and it forwards the record to the
//! shellbridge, which files it into `stage/notifications.json` for the QML notifications to
//! draw. The reading side is the ledger file itself, not a command.

use aoide_protocol::cmd;
use aoide_protocol::registry::Registry;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["notifications", "push"],
        summary: "File one notification into the notifications ledger (stage/notifications.json) for the Quickshell notifications to draw. Reads the DUNST_* environment dunst's `script` hook sets — this is the daemon-to-desktop feed, not a way to send a notification (use `notify-send` for that). Forwarded through the shellbridge so concurrent notifications cannot race each other's writes.",
        args: [],
        flags: [],
        gated: false,
        implemented: true,
        handler: crate::notifications::notifications_push,
        brief: "File one notification into the notifications ledger.",
    ));
}
