// AoideIpc.qml — external IPC endpoint for the Aoide CLI.
//
// Exposes `quickshell ipc call shell reload` (Quickshell.Io.IpcHandler): the
// only way an outside process can trigger a QML-visible change without a
// full `systemctl --user restart aoide-quickshell.service`. `aoide shell
// reload` (crates/song/src/ipc.rs) shells out to exactly this call.
//
// `Quickshell.reload(hard: bool)` (rootwrapper.cpp's reloadGraph()) always
// tears down and rebuilds the WHOLE scene fresh from shell.qml — it is not
// a selective "reload changed files" mechanism, closer to an in-process
// restart. `hard` only controls whether `PersistentProperties`-marked QML
// state survives the rebuild; this codebase declares none, so `hard:true`
// vs `hard:false` is currently observably identical. Quickshell's own
// built-in auto-reload-on-file-change path calls `reload(false)` — soft —
// so this matches that, not `true`.
//
// ── Placeholder-screen diagnostic ──────────────────────────────────────
// `Quickshell.screens`/`screensChanged` (confirmed against the compiled
// 0.3.1 plugin's own quickshell-core.qmltypes: `QuickshellGlobal`'s
// `screens` property, `notify: "screensChanged"`) is logged on every
// change so the journal shows the screen list collapsing at the moment a
// placeholder-screen lockup happens, not only after the fact from `hyprctl`.
// A surface bound to `Quickshell.screens` (the bar, the wallpaper) or to
// the focused monitor (the dock) re-homes by itself: the list came back on
// its own, nothing needed resetting, and only unbound singletons failed to
// follow. A process restart remains the recovery for a QPA state that
// genuinely wedges and paints nothing anywhere. `console.warn`, not a state
// change: zero behavior difference from this file's IPC responsibility above.

import QtQuick
import Quickshell
import Quickshell.Io

IpcHandler {
    target: "shell"
    function reload(): void { Quickshell.reload(false) }

    property Connections _screensWatch: Connections {
        target: Quickshell
        function onScreensChanged() {
            console.warn("[aoide-ipc] Quickshell.screens changed: " + Quickshell.screens.length + " screen(s)")
        }
    }
}
