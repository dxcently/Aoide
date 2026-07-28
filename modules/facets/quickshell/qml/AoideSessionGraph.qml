// AoideSessionGraph.qml — the Session-Graph (DAG) surface (surface #9).
//
// A toggleable overlay that renders the project/session DAG staged by
// `aoide graph emit` at song/stage/graph.json. Projects are ◆ roots, agent
// sessions are ● leaves; spawned sessions nest under their parent, and
// sessions with no incoming edge group under a synthetic "(unanchored)" root.
//
// Reading discipline (CONTRACTS.md §1 / entities/Quickshell):
//   - Hot-reloads from song/stage/graph.json via a FileView, exactly like
//     DrachmaState watches drachma.json (atomic write-temp-then-rename → onTextChanged
//     fires → bindings recompute in one pass). Tolerant of the file being
//     absent: empty state prompts `aoide graph emit`.
//   - Colors come ENTIRELY from notes (no hardcoded hex). State → color map:
//       running                         → paletteAccent
//       awaiting / Notification-ish     → paletteUrgent
//       done                            → paletteFg dimmed (0.5 opacity)
//       other (idle, …)                 → paletteFg
//
// Communication discipline:
//   - Row click → jump to that session's terminal via the shellbridge gate
//     (bridge.focusSession(windowAddress)), the same session-jump path the
//     Terminal-Commander chips use. shellbridge dispatches `aoide graph focus`
//     / `hyprctl dispatch focuswindow` — QML never shells out directly.
//   - Visibility is a bridge-driven toggle (`visible_`), mirroring AoideLauncher.
//     Hidden by default.
//
// STATUS (v1): DORMANT / bridge-only. This standalone overlay no longer owns a
// keybind — SUPER+G now summons the LEFT-edge gadget-dock popup (AoideAgentWidgets),
// which CONTAINS the DAG gadget and is the primary DAG affordance. This file is
// kept intact (not deleted) as the seam for a future dedicated full-screen DAG
// view; it flips `visible_` only if a shellbridge bind is re-added.

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

Item {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ─────────────────
    required property var notes
    required property var bridge

    // ── Visibility gate ────────────────────────────────────────────────────
    // Mirrors AoideLauncher: bridge/shellbridge sends { cmd: "graph",
    // action: "show|hide|toggle" } and flips this flag. Hidden by default.
    property bool visible_: false

    // ── Parsed graph document (v0: { schemaVersion, nodes, edges }) ────────
    // Empty default → the overlay shows the "no graph yet" prompt until the
    // stage file appears.
    property var graph: ({ "schemaVersion": "0", "nodes": [], "edges": [] })

    // ── Graph path ─────────────────────────────────────────────────────────
    // Stage path: ~/Aoide/song/stage/graph.json (gitignored runtime; the nix
    // build never depends on it — checks.no-song-read enforces that).
    readonly property string graphPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/graph.json"

    // ── Row model: flattened, indented tree ────────────────────────────────
    // The DAG-flattening logic (buildRows) is CANONICAL in GraphModel.qml so
    // the always-on dock gadget (DagGraphGadget) shares one implementation.
    // We feed our parsed `graph` in and read `rows` back out.
    GraphModel {
        id: graphModel
        graph: root.graph
    }
    readonly property var rows: graphModel.rows

    // ── Graph overlay ──────────────────────────────────────────────────────
    Rectangle {
        anchors.centerIn: parent
        width: 560
        height: 460
        radius: 0
        color: notes.paletteBg
        border.color: notes.paletteAccent
        border.width: 1
        visible: root.visible_

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 8

            // ── Title ────────────────────────────────────────────────────
            Text {
                text: "Session Graph"
                color: notes.paletteAccent
                font.pixelSize: 15
                font.bold: true
                Layout.fillWidth: true
            }

            // ── Empty state ──────────────────────────────────────────────
            Text {
                text: "no graph yet — run aoide graph emit"
                color: notes.paletteFg
                opacity: 0.6
                font.pixelSize: 13
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                visible: root.rows.length === 0
            }

            // ── DAG tree ─────────────────────────────────────────────────
            ListView {
                id: treeList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 2
                visible: root.rows.length > 0
                model: root.rows
                // Wrapper Item declares required modelData — GraphRow itself
                // has required properties, which disables implicit modelData
                // injection into the delegate root (Qt6 rule); the wrapper
                // captures it and hands it to GraphRow.row.
                delegate: Item {
                    id: rowWrap
                    required property var modelData
                    width: treeList.width
                    implicitHeight: gr.implicitHeight
                    GraphRow {
                        id: gr
                        width: parent.width
                        notes: root.notes
                        row: rowWrap.modelData
                        // Click a session row → jump to its terminal via the
                        // shellbridge session-jump gate (same path as bar chips).
                        onActivate: function (windowAddress) {
                            if (windowAddress)
                                root.bridge.focusSession(windowAddress)
                        }
                    }
                }
            }
        }
    }

    // ── File watcher — atomic hot-reload (mirrors DrachmaState) ───────────────
    // Tolerant of graph.json being absent: reload() on a missing path leaves
    // `graph` at its empty default, so the overlay shows the empty prompt.
    FileView {
        id: graphFile
        path: root.graphPath
        watchChanges: true
        onFileChanged: graphFile.reload()
        onTextChanged: {
            try {
                root.graph = JSON.parse(graphFile.text())
            } catch (err) {
                console.warn("[aoide/graph] Failed to parse graph.json:", err)
            }
        }
        Component.onCompleted: graphFile.reload()
    }
}
