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

import QtQuick
import Quickshell
import Quickshell.Io

IpcHandler {
    target: "shell"
    function reload(): void { Quickshell.reload(false) }
}
