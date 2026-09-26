// Kit.js — cadenza's shared kit: colour roles, the character cell, the
// glyph builders, and the URLs of the helper files.
//
// Sharing mechanism (design/kit.md §1): NOTHING in cadenza is resolved by
// type name across files. A sibling `Pane { }` needs a per-song qmldir, which
// the nix build writes but `lyra preview` and `rice stage` hot-sync do not —
// cadenza goes live through hot-sync, so a name-resolved helper would not be
// a type there. Instead:
//   · this file is imported by relative URL — `import "Kit.js" as Kit` — a
//     direct fetch of one file, which needs no directory listing and no qmldir;
//   · each visual helper (Pane.qml, Gauge.qml, …) is instantiated by URL
//     through a Loader: `Loader { Component.onCompleted:
//     setSource(Kit.helper("Pane"), { … }) }` — SessionMenu.qml's precedent.
//
// Every widget builds ONE kit object at its root and passes it down:
//
//     import "Kit.js" as Kit
//     FontMetrics { id: fm; font: Kit.bodyFont(13) }
//     readonly property var kit: Kit.make(root.livery, fm)
//
// The binding re-evaluates when the livery or the metrics change (QML tracks
// the property reads made inside make()), so a palette swap recolours live.
.pragma library

var family = "JetBrainsMono Nerd Font Mono"

function bodyFont(px)  { return Qt.font({ family: family, pixelSize: px }) }
function titleFont(px) { return Qt.font({ family: family, pixelSize: px, bold: true }) }

// the URL of a helper file sitting beside this one
function helper(name) { return Qt.resolvedUrl(name + ".qml") }

function withA(c, a) { var q = Qt.color(c); return Qt.rgba(q.r, q.g, q.b, a) }

// ══ THE KIT OBJECT ═════════════════════════════════════════════════════════
// livery: the LiveryState a widget is handed. fm: a FontMetrics on bodyFont.
function make(livery, fm) {
    var b = (livery && livery.raw && livery.raw.base16) ? livery.raw.base16 : {}
    function slot(n, fb) { return b[n] ? b[n] : fb }
    var px = fm.font.pixelSize
    var k = {
        // ── colour roles: one job each (intent §2) ─────────────────────────
        ground: slot("base00", livery.paletteBg),   // CRT black; pane fill at paneAlpha
        raised: slot("base01", livery.paletteBg),   // borderless-block fill
        select: slot("base02", livery.paletteBg),   // selected / focused row band
        dim:    slot("base03", livery.paletteFg),   // axes, labels, rules at rest, timestamps
        mid:    slot("base04", livery.paletteFg),   // secondary text, idle lamps
        ink:    livery.paletteFg,                   // body text, idle rules, chart ink (base05)
        bright: slot("base06", livery.paletteFg),   // bright phosphor text
        match:  slot("base07", livery.paletteFg),   // white-hot: a search match / selected row text
        title:  livery.paletteAccent,               // titles, focused rule, active jack (base0B)
        hot:    livery.paletteHot,                  // amber: the ONE live element (base0A)
        urgent: livery.paletteUrgent,               // red: blocked, failed, a summons on you (base08)
        warn:   livery.base09,                      // orange: past a threshold, costPartial
        number: livery.wireCyan,                    // cyan: counts, tokens, durations (base0C)
        path:   livery.holoBlue,                    // blue: paths, project names, ws numbers (base0D)
        human:  livery.violet,                      // magenta: a person's words (base0E)
        paneAlpha: 0.94,

        // ── the cell ──────────────────────────────────────────────────────
        font: bodyFont(px),
        titleFont: titleFont(px),
        cellW: Math.ceil(fm.advanceWidth("M") * 100) / 100,
        cellH: Math.ceil(fm.height),

        // ── functions (so a helper needs only the object) ─────────────────
        withA: withA, helper: helper,
        gaugeText: gaugeText, sparkText: sparkText, brailleLines: brailleLines,
        barColumn: barColumn, padL: padL, padR: padR, lampGlyph: lampGlyph,
        rep: rep, clamp01: clamp01
    }
    k.cells = function(n) { return Math.round(n * k.cellW) }
    k.lines = function(n) { return Math.round(n * k.cellH) }
    k.fit   = function(px) { return Math.max(0, Math.floor(px / k.cellW)) }
    k.lampColor = function(state) {
        switch (state) {
        case "working":  return k.title
        case "awaiting": return k.urgent
        case "failed":   return k.urgent
        case "stopped":  return k.ink
        case "idle":     return k.mid
        default:         return k.dim
        }
    }
    return k
}

// ══ STATE LAMPS ════════════════════════════════════════════════════════════
// the daemon's canonical live states (sonata widget-structure §8) + failed;
// an unknown state is a dim `·`, never a guess
var lamps = { "working": "●", "awaiting": "◐", "idle": "○", "stopped": "■", "failed": "✕" }
function lampGlyph(state) { return lamps[state] || "·" }

// ══ GLYPH BUILDERS — numbers in, strings out ═══════════════════════════════
var eighthsH = " ▏▎▍▌▋▊▉█"   // 0..8, left-anchored
var eighthsV = " ▁▂▃▄▅▆▇█"   // 0..8, bottom-anchored

function rep(ch, n) { var s = ""; for (var i = 0; i < n; i++) s += ch; return s }
function clamp01(v) { return (v === null || v === undefined || isNaN(v)) ? 0 : Math.max(0, Math.min(1, v)) }

// gauge → { fill, track }: the lit run (whole █ + one eighth) and the ░ rest
function gaugeText(frac, width) {
    var f = clamp01(frac) * width
    var whole = Math.floor(f), eighth = Math.round((f - whole) * 8)
    if (eighth === 8) { whole += 1; eighth = 0 }
    var fill = rep("█", whole) + (eighth > 0 && whole < width ? eighthsH.charAt(eighth) : "")
    return { fill: fill, track: rep("░", Math.max(0, width - fill.length)) }
}

// sparkline: the last `width` samples, one ▁…█ column each, right-aligned;
// `max` <= 0 scales to the window's own peak; a null sample is a space
function sparkText(values, width, max) {
    var v = values || []
    var start = Math.max(0, v.length - width)
    var peak = max > 0 ? max : 0
    if (peak <= 0) for (var i = start; i < v.length; i++) peak = Math.max(peak, v[i] || 0)
    var s = ""
    for (var j = start; j < v.length; j++) {
        var x = v[j]
        if (x === null || x === undefined || isNaN(x)) { s += " "; continue }
        s += "▁▂▃▄▅▆▇█".charAt(peak > 0 ? Math.round(clamp01(x / peak) * 7) : 0)
    }
    return rep(" ", Math.max(0, width - s.length)) + s
}

// braille line chart: cols × rows cells, 2 × 4 dots per cell; the series is
// resampled to 2·cols points and each column is joined vertically to the
// previous one so it reads as a line. `rows` strings, top first.
function brailleLines(values, cols, rows, lo, hi) {
    var v = values || []
    var W = cols * 2, H = rows * 4
    var grid = []
    for (var r = 0; r < rows; r++) { grid.push([]); for (var c = 0; c < cols; c++) grid[r].push(0) }
    if (v.length > 0) {
        var autoLo = (lo === undefined || lo === null), autoHi = (hi === undefined || hi === null)
        var min = autoLo ? Infinity : lo, max = autoHi ? -Infinity : hi
        for (var i = 0; i < v.length; i++) {
            if (autoLo) min = Math.min(min, v[i])
            if (autoHi) max = Math.max(max, v[i])
        }
        var span = (max - min) || 1
        var bits = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]]
        var yAt = function(x) {
            var t = W === 1 ? 0 : x / (W - 1) * (v.length - 1)
            var a = Math.floor(t), b = Math.min(v.length - 1, a + 1), f = t - a
            var val = v[a] * (1 - f) + v[b] * f
            return Math.max(0, Math.min(H - 1, Math.round((1 - (val - min) / span) * (H - 1))))
        }
        var prev = -1
        for (var x = 0; x < W; x++) {
            var y = yAt(x)
            var from = prev < 0 ? y : (y > prev ? prev + 1 : y)
            var to   = prev < 0 ? y : (y > prev ? y : Math.max(y, prev - 1))
            for (var yy = from; yy <= to; yy++)
                grid[Math.floor(yy / 4)][Math.floor(x / 2)] |= bits[yy % 4][x % 2]
            prev = y
        }
    }
    return grid.map(function(row) {
        return row.map(function(bb) { return String.fromCharCode(0x2800 + bb) }).join("")
    })
}

// one bar-chart column: `rows` strings top first — full blocks, an
// eighth-block cap, blank above
function barColumn(frac, rows) {
    var f = clamp01(frac) * rows
    var whole = Math.floor(f), eighth = Math.round((f - whole) * 8)
    if (eighth === 8) { whole += 1; eighth = 0 }
    var out = []
    for (var r = rows - 1; r >= 0; r--) {
        if (r < whole) out.push("█")
        else if (r === whole && eighth > 0) out.push(eighthsV.charAt(eighth))
        else out.push(" ")
    }
    return out
}

// exactly n cells: padL right-aligns (clips), padR left-aligns (clips with …)
function padL(s, n) { s = String(s); return s.length >= n ? s.slice(0, n) : rep(" ", n - s.length) + s }
function padR(s, n) { s = String(s); return s.length > n ? s.slice(0, Math.max(0, n - 1)) + "…" : s + rep(" ", n - s.length) }
