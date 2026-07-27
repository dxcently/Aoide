// MeterGadget.qml — CPU / RAM gauges as ASCII bars ([▓▓▓░░░] NN%).
//
// A gadget for the right-edge dock (AoideAgentWidgets). Two rows: CPU and RAM,
// each an ASCII gauge bar filled proportionally + a percentage. Monospace, all
// colors from notes (zero hardcoded hex).
//
// Data-source discipline (no-shell-out rule): QML never spawns a process. There
// is no existing system-telemetry stage file in the repo (the bar/OSD skeletons
// only ever bind to shellbridge-written stage JSON, and shellbridge writes none
// for CPU/RAM). So — per the task's explicit fallback — we read the kernel's
// /proc pseudo-files directly via FileView (files, not processes → within the
// rule) and compute the CPU busy-fraction as a DELTA between two samples. RAM is
// a point-in-time ratio from /proc/meminfo (no delta needed).
//
// If a system-telemetry stage file is ever introduced, this gadget should switch
// to watching it (same FileView pattern) so all data flows through one seam.

import QtQuick
import Quickshell.Io

Item {
    id: root

    required property var notes

    // ── Sampling cadence ───────────────────────────────────────────────────
    property int sampleMs: 2000

    // ── Computed gauges (0..100) ───────────────────────────────────────────
    property int cpuPercent: 0
    property int ramPercent: 0

    // ── CPU delta state (previous /proc/stat aggregate cpu line) ───────────
    // We keep the previous (total, idle) so each sample is busy_delta/total_delta.
    property double prevTotal: -1
    property double prevIdle: -1

    implicitHeight: column.implicitHeight

    // ── ASCII gauge renderer ───────────────────────────────────────────────
    // Fixed 10-cell bar: [▓▓▓▓▓░░░░░]. Machine-checked (see node snippets).
    function gaugeBar(percent) {
        var cells = 10
        var p = percent
        if (p < 0) p = 0
        if (p > 100) p = 100
        var filled = Math.round(p / 100 * cells)
        if (filled > cells) filled = cells
        var s = "["
        for (var i = 0; i < cells; i++)
            s += (i < filled) ? "▓" : "░"
        s += "]"
        return s
    }

    // ── /proc/stat parse → { total, idle } for the aggregate "cpu " line ───
    // Line: "cpu  user nice system idle iowait irq softirq steal guest guest_nice"
    // idle bucket = idle + iowait; total = sum of all numeric fields.
    function parseCpu(text) {
        if (!text)
            return null
        var lines = text.split("\n")
        for (var i = 0; i < lines.length; i++) {
            var ln = lines[i]
            if (ln.indexOf("cpu ") === 0 || ln.indexOf("cpu\t") === 0) {
                // Split on runs of whitespace; drop the leading "cpu" label.
                var parts = ln.trim().split(/\s+/)
                var nums = []
                for (var j = 1; j < parts.length; j++) {
                    var v = parseInt(parts[j], 10)
                    if (!isNaN(v)) nums.push(v)
                }
                if (nums.length < 4)
                    return null
                var idle = nums[3] + (nums.length > 4 ? nums[4] : 0) // idle + iowait
                var total = 0
                for (var k = 0; k < nums.length; k++) total += nums[k]
                return { "total": total, "idle": idle }
            }
        }
        return null
    }

    // ── /proc/meminfo parse → used% = (MemTotal - MemAvailable)/MemTotal ───
    function parseMem(text) {
        if (!text)
            return -1
        var total = -1, avail = -1
        var lines = text.split("\n")
        for (var i = 0; i < lines.length; i++) {
            var ln = lines[i]
            if (ln.indexOf("MemTotal:") === 0)
                total = parseInt(ln.replace(/[^0-9]/g, ""), 10)
            else if (ln.indexOf("MemAvailable:") === 0)
                avail = parseInt(ln.replace(/[^0-9]/g, ""), 10)
            if (total >= 0 && avail >= 0)
                break
        }
        if (total <= 0 || avail < 0)
            return -1
        var used = total - avail
        if (used < 0) used = 0
        return Math.round(used / total * 100)
    }

    function sampleCpu(text) {
        var cur = parseCpu(text)
        if (!cur)
            return
        if (root.prevTotal >= 0) {
            var dTotal = cur.total - root.prevTotal
            var dIdle = cur.idle - root.prevIdle
            if (dTotal > 0) {
                var busy = (dTotal - dIdle) / dTotal * 100
                if (busy < 0) busy = 0
                if (busy > 100) busy = 100
                root.cpuPercent = Math.round(busy)
            }
        }
        root.prevTotal = cur.total
        root.prevIdle = cur.idle
    }

    Column {
        id: column
        width: parent ? parent.width : implicitWidth
        spacing: 3

        // ── CPU gauge ─────────────────────────────────────────────────────
        Row {
            spacing: 6
            Text {
                text: "CPU"
                color: notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 12
                width: 28
            }
            Text {
                text: root.gaugeBar(root.cpuPercent)
                // Accent when hot-ish, plain fg otherwise — still notes-only.
                color: root.cpuPercent >= 85 ? notes.paletteUrgent : notes.paletteAccent
                font.family: "monospace"
                font.pixelSize: 12
            }
            Text {
                text: (root.cpuPercent < 10 ? " " : "") + root.cpuPercent + "%"
                color: notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 12
            }
        }

        // ── RAM gauge ─────────────────────────────────────────────────────
        Row {
            spacing: 6
            Text {
                text: "RAM"
                color: notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 12
                width: 28
            }
            Text {
                text: root.gaugeBar(root.ramPercent)
                color: root.ramPercent >= 85 ? notes.paletteUrgent : notes.paletteAccent
                font.family: "monospace"
                font.pixelSize: 12
            }
            Text {
                text: (root.ramPercent < 10 ? " " : "") + root.ramPercent + "%"
                color: notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 12
            }
        }
    }

    // ── /proc/stat watcher (CPU) ───────────────────────────────────────────
    // FileView reads the file; a Timer forces a reload each sampleMs so the
    // delta advances. Absent/garbage → parse returns null → gauge holds.
    FileView {
        id: statFile
        path: "/proc/stat"
        onTextChanged: root.sampleCpu(statFile.text())
    }

    // ── /proc/meminfo watcher (RAM) ────────────────────────────────────────
    FileView {
        id: memFile
        path: "/proc/meminfo"
        onTextChanged: {
            var p = root.parseMem(memFile.text())
            if (p >= 0)
                root.ramPercent = p
        }
    }

    Timer {
        interval: root.sampleMs
        repeat: true
        running: true
        onTriggered: {
            statFile.reload()
            memFile.reload()
        }
    }

    Component.onCompleted: {
        statFile.reload()
        memFile.reload()
    }
}
