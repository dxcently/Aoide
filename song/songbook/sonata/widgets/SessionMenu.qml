// Session actions share one square paper menu; the bridge owns every mutation.
import QtQuick
import QtQuick.Dialogs
import Quickshell
import Quickshell.Io

Item {
    id: menu
    readonly property string stateDir: {
        var path = Quickshell.env("AOIDE_STATE_DIR") || ""
        return path.charAt(0) === "/" ? path : (Quickshell.env("AOIDE_ROOT") || Quickshell.env("HOME") + "/.aoide") + "/state"
    }
    readonly property string stageDir: {
        var path = Quickshell.env("AOIDE_STAGE_DIR") || ""
        return path.charAt(0) === "/" ? path : stateDir + "/stage"
    }
    property var livery
    property var bridge
    property var record: ({})
    property var host: null
    property string page: "actions"
    property bool editing: false
    property string message: ""
    property bool failed: false
    property bool busy: false
    property var projects: []
    property var paths: []
    property var undyingIds: []
    property real pointX: 0
    property real pointY: 0
    visible: false
    property string face: "JetBrainsMono Nerd Font"
    readonly property string recovery: recoveryFor(record)
    function recoveryFor(rec) {
        rec = rec || {}
        return "Session: " + (rec.sessionId || "unavailable")
        + "\nNative session: " + (rec.harnessSessionId || "unavailable")
        + "\nPID: " + (rec.pid || "unavailable")
        + "\nWindow: " + (rec.windowAddress || "unavailable")
        + "\nTitle: " + (rec.title || "unavailable")
        + "\nPetname: " + (rec.petname || "unavailable")
        + "\nHarness: " + (rec.agent || "unavailable")
        + "\nModel: " + (rec.model || "unavailable")
        + "\nHost: " + (rec.host || "unavailable")
        + "\nDirectory: " + (rec.cwd || "unavailable")
        + "\nState: " + (rec.state || "unavailable")
        + "\nProject: " + effectiveProjectOf(rec)
        + "\nPrompt: " + (rec.prompt || "unavailable")
    }
    function open(rec, x, y, host, page) {
        if (busy) return
        record = JSON.parse(JSON.stringify(rec || {}))
        menu.host = host || null
        undyingFile.reload()
        record.undying = undyingIds.indexOf(record.sessionId) >= 0
        pointX = x; pointY = y; menu.page = page || "actions"; editing = false; message = ""; failed = false
        nameInput.text = ""; pathInput.text = ""; paths = record.cwd ? [record.cwd] : []
        visible = true; forceActiveFocus(); projectFile.reload()
    }
    // the plaques' copy chip — the same recovery text the menu copies, menu kept closed
    function copyRecovery(rec) {
        clipText.text = recoveryFor(rec)
        clipText.selectAll(); clipText.copy(); clipText.deselect()
    }
    // the plaques' clipboard courier — never shown, never the open sheet's
    // record, so a copy from a card cannot retarget a sheet already open
    TextEdit { id: clipText; visible: false; readOnly: true; textFormat: TextEdit.PlainText }
    function clearUndying() {
        undyingIds = []
        var rec = JSON.parse(JSON.stringify(record)); rec.undying = false; record = rec
    }
    // The kill Action names its scope before the click (P-QOL-UI1): a
    // subagent shares its executor's process and never resolves on its own,
    // a record that already owns a dedicated process kills itself, a hosted
    // record names the immediate parent conductor.qml resolved as `host`,
    // and anything else defers to aoide's own ancestor walk
    // (`conduct::kill_target`) rather than guess.
    function killLabel() {
        var rec = menu.record || {}
        if (rec.kind === "app") return "Kill unavailable (app-owned)"
        if (rec.state === "done") return "Kill unavailable (ended)"
        if (rec.kind === "subagent" || ("" + (rec.sessionId || "")).indexOf("sub:") === 0)
            return "Kill unavailable (subagent)"
        if (!killEnabled()) return "Kill unavailable (state unknown)"
        if (rec.conductable === true && rec.pid) return "Kill process"
        if (menu.host && menu.host.conductable === true && menu.host.pid && menu.host.state !== "done")
            return "Kill terminal " + (menu.host.petname || menu.host.title || menu.host.sessionId)
        return "Kill…"
    }
    function killHint() {
        var rec = menu.record || {}
        if (rec.kind === "app")
            return "The app owns this task; Aoide has no task-specific stop action."
        if (rec.state === "done") return "This session has already ended."
        if (rec.kind === "subagent" || ("" + (rec.sessionId || "")).indexOf("sub:") === 0)
            return "A subagent shares its executor's process; kill the executor instead."
        if (!killEnabled()) return "No live session state is available."
        if (rec.conductable === true && rec.pid) return "Stops this terminal's own process."
        if (menu.host && menu.host.conductable === true && menu.host.pid && menu.host.state !== "done")
            return "Stops the whole terminal that hosts this session."
        return "aoide resolves the hosting terminal on click and refuses if there is none."
    }
    function killEnabled() {
        var rec = menu.record || {}
        return rec.kind !== "app" && rec.kind !== "subagent"
            && ("" + (rec.sessionId || "")).indexOf("sub:") !== 0
            && (rec.state === "working" || rec.state === "awaiting"
                || rec.state === "stopped" || rec.state === "idle")
    }
    function effectiveProjectOf(rec) {
        if (rec.project) return rec.project
        var best = "", length = -1, cwd = rec.cwd || ""
        for (var i = 0; i < projects.length; i++) {
            var roots = projects[i].roots && projects[i].roots.length ? projects[i].roots : [projects[i].path]
            for (var j = 0; j < roots.length; j++) {
                var p = roots[j]
                if (p && (cwd === p || cwd.indexOf(p === "/" ? "/" : p + "/") === 0) && p.length > length) {
                    best = projects[i].name; length = p.length
                }
            }
        }
        return best ? best + " (automatic)" : "No project"
    }
    function editProject(project) {
        editing = true; nameInput.text = project.name
        paths = project.roots && project.roots.length ? project.roots.slice() : (project.path ? [project.path] : [])
        pathInput.text = ""; message = ""; page = "edit project"
    }
    function addPath(path) {
        if (path.charAt(0) !== "/") { failed = true; message = "Choose an absolute directory."; return }
        if (paths.indexOf(path) < 0) paths = paths.concat([path])
        pathInput.text = ""; message = ""; failed = false
    }
    FolderDialog {
        id: directoryPicker
        title: "Add a project directory"
        onAccepted: menu.addPath(decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, "")))
    }
    function run(action, fields) {
        if (busy) return
        if (action === "createproject") {
            for (var i = 0; i < projects.length; i++) if (projects[i].name === fields.name) {
                failed = true; message = "This project exists. Edit its directories from Project instead."; return
            }
        }
        if (!bridge || !bridge.sessionAction) {
            failed = true; message = "Session actions unavailable; update the running shell bridge."; return
        }
        busy = true; failed = false; message = "Waiting for Aoide…"
        bridge.sessionAction(record.sessionId || "", action, fields, function(result) {
            busy = false; failed = !result.ok
            message = result.message || (result.ok ? "Done." : "Action failed.")
            if (result.ok && action === "undying") {
                var next = JSON.parse(JSON.stringify(record)); next.undying = fields.state === "on"; record = next
                undyingFile.reload()
            }
            if (result.ok && (action === "project" || action === "createproject")) {
                var next = JSON.parse(JSON.stringify(record)); next.project = fields.project || fields.name || ""; record = next
                projectFile.reload(); page = "actions"
            }
            if (result.ok && action === "editproject") { projectFile.reload(); page = "projects" }
        })
    }
    Keys.onEscapePressed: visible = false
    FileView {
        id: undyingFile
        onLoadFailed: menu.clearUndying()
        path: menu.stateDir + "/undying.json"
        watchChanges: true; printErrors: false
        onFileChanged: reload()
        onLoaded: {
            try {
                var data = JSON.parse(text())
                menu.undyingIds = (data.undying || data.carried || []).map(function(r) { return r.sessionId })
                if (!menu.busy) { var rec = JSON.parse(JSON.stringify(menu.record)); rec.undying = menu.undyingIds.indexOf(rec.sessionId) >= 0; menu.record = rec }
            } catch (e) { menu.clearUndying() }
        }
    }
    FileView {
        id: projectFile
        path: menu.stageDir + "/projects.json"
        watchChanges: true; printErrors: false
        onFileChanged: reload()
        onLoaded: { try { menu.projects = JSON.parse(text()).projects || [] } catch (e) { menu.projects = [] } }
    }
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton; onClicked: menu.visible = false }
    Rectangle {
        id: sheet
        x: Math.max(5, Math.min(menu.pointX, menu.width - width - 5))
        y: Math.max(5, Math.min(menu.pointY, menu.height - height - 5))
        width: Math.min(292, menu.width - 10)
        height: Math.min(content.implicitHeight + 20, menu.height - 10)
        radius: 0; color: menu.livery ? menu.livery.paletteBg : "transparent"
        border.width: 2; border.color: menu.livery ? menu.livery.paletteFg : "transparent"
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton }
        Flickable {
            anchors.fill: parent; anchors.margins: 10; clip: true
            contentHeight: content.implicitHeight; boundsBehavior: Flickable.StopAtBounds
            Column {
                id: content
                width: parent.width; spacing: 4
                Text {
                    width: parent.width; text: menu.record.petname || menu.record.title || menu.record.agent || "session"
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    font.family: "Noto Serif"; font.pixelSize: 13; font.bold: true
                    color: menu.livery ? menu.livery.paletteFg : "transparent"
                }
                Text { width: parent.width; text: menu.page === "actions" ? "┌─ session actions ─┐" : "┌─ " + menu.page + " ─┐"; font.family: menu.face; font.pixelSize: 10; color: menu.livery ? menu.livery.paletteAccent : "transparent" }
                Action { visible: menu.page !== "actions"; label: "‹ back"; onChosen: { menu.page = "actions"; menu.message = "" } }
                Column {
                    width: parent.width; visible: menu.page === "actions"; spacing: 2
                    Action { label: "Copy recovery information"; onChosen: { recoveryText.selectAll(); recoveryText.copy(); recoveryText.deselect(); menu.message = "Recovery information copied."; menu.failed = false } }
                    Action { label: "Details"; onChosen: menu.page = "details" }
                    Action { label: (menu.record.undying === true ? "[x]" : "[ ]") + " Undying"; onChosen: menu.run("undying", {state: menu.record.undying === true ? "off" : "on"}) }
                    Text { width: parent.width; text: "Keep for manual resurrection."; font.family: menu.face; font.pixelSize: 9; wrapMode: Text.WordWrap; color: menu.livery ? menu.livery.paletteFg : "transparent"; opacity: 0.55 }
                    Action { label: "Project…"; onChosen: menu.page = "projects" }
                    Action { label: menu.killLabel(); danger: true; enabled: !menu.busy && menu.killEnabled(); onChosen: menu.run("kill", {}) }
                    Text { width: parent.width; text: menu.killHint(); font.family: menu.face; font.pixelSize: 9; wrapMode: Text.WordWrap; color: menu.livery ? menu.livery.paletteFg : "transparent"; opacity: 0.55 }
                }
                TextEdit {
                    id: recoveryText
                    width: parent.width; height: visible ? contentHeight : 0
                    visible: menu.page === "details"; text: menu.recovery
                    textFormat: TextEdit.PlainText; readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere
                    font.family: menu.face; font.pixelSize: 10; color: menu.livery ? menu.livery.paletteFg : "transparent"
                }
                Column {
                    width: parent.width; visible: menu.page === "projects"; spacing: 2
                    Action { label: "Automatic from directory"; onChosen: menu.run("project", {project: ""}) }
                    Repeater {
                        model: menu.projects
                        delegate: Column {
                            required property var modelData
                            width: parent.width
                            Action {
                                label: (menu.record.project === modelData.name ? "[x] " : "") + modelData.name
                                onChosen: menu.run("project", {project: modelData.name})
                            }
                            Action { label: "  Edit directories…"; onChosen: menu.editProject(modelData) }
                        }
                    }
                    Action { label: "Create project…"; onChosen: { menu.editing = false; nameInput.text = ""; menu.paths = menu.record.cwd ? [menu.record.cwd] : []; menu.page = "create project" } }
                }
                Column {
                    width: parent.width; visible: menu.page === "create project" || menu.page === "edit project"; spacing: 6
                    Text { width: parent.width; text: menu.editing ? "Edit this project’s directory roots. Session assignments stay unchanged." : "Register project directories and assign this session. Directories themselves are not created."; textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.family: menu.face; font.pixelSize: 10; color: menu.livery ? menu.livery.paletteFg : "transparent" }
                    Text { text: "Name"; font.family: menu.face; font.pixelSize: 10; color: menu.livery ? menu.livery.paletteAccent : "transparent" }
                    Rectangle {
                        width: parent.width; height: 26; color: "transparent"; border.width: 1; border.color: menu.livery ? menu.livery.paletteAccent : "transparent"
                        TextInput { id: nameInput; readOnly: menu.editing; anchors.fill: parent; anchors.margins: 4; clip: true; selectByMouse: true; font.family: menu.face; font.pixelSize: 11; color: menu.livery ? menu.livery.paletteFg : "transparent" }
                    }
                    Repeater {
                        model: menu.paths
                        delegate: Action {
                            required property string modelData
                            required property int index
                            label: "[-] " + modelData
                            onChosen: { var next = menu.paths.slice(); next.splice(index, 1); menu.paths = next }
                        }
                    }
                    Action { label: "Browse for another directory…"; onChosen: directoryPicker.open() }
                    Text { text: "Or enter an absolute directory"; font.family: menu.face; font.pixelSize: 10; color: menu.livery ? menu.livery.paletteAccent : "transparent" }
                    Rectangle {
                        width: parent.width; height: 26; color: "transparent"; border.width: 1; border.color: menu.livery ? menu.livery.paletteAccent : "transparent"
                        TextInput { id: pathInput; anchors.fill: parent; anchors.margins: 4; clip: true; selectByMouse: true; font.family: menu.face; font.pixelSize: 11; color: menu.livery ? menu.livery.paletteFg : "transparent" }
                    }
                    Action { label: "Add directory"; enabled: !menu.busy && pathInput.text.charAt(0) === "/"; onChosen: menu.addPath(pathInput.text) }
                    Action { label: menu.editing ? "Save directories" : "Create and assign"; enabled: !menu.busy && nameInput.text.trim() !== "" && menu.paths.length > 0; onChosen: menu.run(menu.editing ? "editproject" : "createproject", {name: nameInput.text.trim(), paths: menu.paths}) }
                }
                Text { width: parent.width; visible: text !== ""; text: menu.message; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; font.family: menu.face; font.pixelSize: 10; color: menu.livery ? (menu.failed ? menu.livery.paletteUrgent : menu.livery.paletteAccent) : "transparent" }
                Action { label: "Close"; enabled: true; onChosen: menu.visible = false }
            }
        }
    }
    component Action: Rectangle {
        property string label
        property bool danger: false
        signal chosen()
        width: parent.width; height: visible ? 26 : 0; radius: 0
        enabled: !menu.busy
        color: hit.containsMouse ? (menu.livery ? Qt.alpha(menu.livery.paletteAccent, 0.12) : "transparent") : "transparent"
        opacity: enabled ? 1 : 0.45
        Text { anchors.fill: parent; anchors.leftMargin: 4; verticalAlignment: Text.AlignVCenter; text: parent.label; textFormat: Text.PlainText; elide: Text.ElideRight; font.family: menu.face; font.pixelSize: 11; color: menu.livery ? (parent.danger ? menu.livery.paletteUrgent : menu.livery.paletteFg) : "transparent" }
        MouseArea { id: hit; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: parent.chosen() }
    }
}
