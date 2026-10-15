// AoideClipboard.qml — the cliphist clipboard-backend for the Grimoire.
//
// Entries are display-only metadata. Copying and image thumbnails call the
// system's own `cliphist` and `wl-copy` from PATH; removing history runs
// `lyra clipboard delete <id>` / `lyra clipboard clear`, the bridge that also
// drops the cached thumbnails. Every one accepts only a validated numeric
// cliphist identifier. Each `sh -c` script is a constant and the id reaches
// it as a positional argument: clipboard contents never become a command or
// shell source.

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    property bool loading: false
    property bool loadError: false
    property string error: ""
    // The last deleteById/clear refusal, as lyra's message; "" when it worked.
    property string actionError: ""
    property var entries: []
    readonly property var parsedEntries: root.entries

    readonly property int previewLimit: 180
    readonly property int maxEntries: 500

    // Per-entry preview state tracked by numeric cliphist id.
    property var imageStates: ({})

    // Keep terminal/control characters out of the rendered preview. Newlines
    // become a visible return marker while the clipboard payload is unchanged.
    function previewOf(value) {
        var source = "" + value
        var out = ""
        var truncated = false
        for (var i = 0; i < source.length; i++) {
            var code = source.charCodeAt(i)
            var token = code === 10 ? " ↵ "
                        : code === 9 ? " ⇥ "
                        : (code < 32 || code === 127) ? "�"
                        : source.charAt(i)
            if (out.length + token.length >= root.previewLimit) {
                truncated = true
                break
            }
            out += token
        }
        if (truncated) out += "…"
        return out
    }

    // Parse cliphist 0.7.0 image-metadata line:
    //   [[ binary data <size> <format> <width>x<height> ]]
    // Returns { format, width, height, metadata } or null.
    function parseImageMeta(raw) {
        // Non-greedy .+? handles multi-word sizes e.g. "1.2 MiB".
        var m = raw.match(/^\[\[\s*binary data\s+.+?\s+(png|jpeg|jpg|gif|bmp|tiff)\s+(\d+)x(\d+)\s*\]\]$/i)
        if (!m) return null
        var fmt = m[1].toLowerCase()
        if (fmt === "jpg") fmt = "jpeg"
        var w = parseInt(m[2], 10)
        var h = parseInt(m[3], 10)
        if (w <= 0 || h <= 0) return null
        var meta = "IMAGE · " + fmt.toUpperCase() + " · " + w + "×" + h
        return { format: fmt, width: w, height: h, metadata: meta }
    }

    // Start decoding the preview image for one numeric cliphist id into
    // previewPath(id). imageStates[id] goes "loading", then "ready" or
    // "error". Anything but digits is refused before a process exists.
    function requestPreview(value) {
        var id = "" + value
        if (!/^[0-9]+$/.test(id)) return
        var st = root.imageStates[id]
        if (!st || st.state === "error" || st.state === "none") {
            if (!st) {
                st = { state: "loading", source: "" }
                root.imageStates[id] = st
            } else {
                st.state = "loading"
            }
            root.imageStatesChanged()
            var proc = previewFactory.createObject(root, { clipId: id })
            proc.running = true
        }
    }

    // Thumbnails are cached per id under the user's XDG cache, so they last
    // as long as the history does. The decoder writes here; previewPath reads.
    readonly property string previewDir: (Quickshell.env("XDG_CACHE_HOME") || (Quickshell.env("HOME") + "/.cache")) + "/aoide-clipboard/previews"

    function previewPath(id) {
        return "file://" + root.previewDir + "/" + id
    }

    function refresh() {
        root.loading = true
        root.loadError = false
        root.error = ""
        root.entries = []
        root.imageStates = ({})
        lister.running = false
        lister.running = true
    }

    function copyById(value) {
        var id = "" + value
        if (!/^[0-9]+$/.test(id)) return false
        // Only the validated identifier is passed, as $1 of a constant
        // script; never preview text, never a command built from contents.
        Quickshell.execDetached(["sh", "-c", "cliphist decode \"$1\" | wl-copy", "sh", id])
        return true
    }

    // Remove one entry and its thumbnail, then reload the list. False when
    // the id is not a numeric cliphist id (nothing runs).
    function deleteById(value) {
        var id = "" + value
        if (!/^[0-9]+$/.test(id)) return false
        root.runLyra(["delete", id])
        return true
    }

    // Wipe the whole history and every thumbnail, then reload the list.
    // Confirming first is the caller's job.
    function clear() {
        root.runLyra(["clear"])
    }

    function runLyra(args) {
        root.actionError = ""
        var proc = lyraFactory.createObject(root, { clipArgs: args })
        proc.running = true
    }

    // One `lyra clipboard <args> --json` run. A failure leaves lyra's own
    // message in actionError; either way the list reloads to what cliphist
    // holds now.
    property Component lyraFactory: Component {
        Process {
            id: lyraRun
            property var clipArgs: []
            command: ["lyra", "clipboard"].concat(clipArgs).concat(["--json"])
            stdout: StdioCollector { id: lyraOut }
            stderr: StdioCollector { id: lyraErr }
            onExited: function (exitCode, exitStatus) {
                if (exitCode !== 0) {
                    // a refusal's JSON arrives on stderr; stdout is the fallback
                    var message = "lyra clipboard " + lyraRun.clipArgs[0] + " failed"
                    try { message = JSON.parse(lyraErr.text || lyraOut.text).message || message } catch (e) {}
                    root.actionError = message
                }
                root.refresh()
                destroy()
            }
        }
    }

    // Factory for one-shot preview decoders — each instance decodes exactly
    // one numeric id and self-destructs. The script is a constant; the id and
    // the cache dir arrive as $1 and $2. It writes a temp file and renames it,
    // so the launcher never reads a partial image, and keeps the cache
    // owner-only (umask 077, dir mode 700).
    property Component previewFactory: Component {
        Process {
            property string clipId: ""
            command: ["sh", "-c",
                "set -e; umask 077; mkdir -p \"$2\"; chmod 700 \"$2\"; "
                + "t=\"$2/.tmp.$1.$$\"; trap 'rm -f \"$t\"' EXIT; "
                + "cliphist decode \"$1\" > \"$t\"; mv -f \"$t\" \"$2/$1\"",
                "sh", clipId, root.previewDir]
            onExited: function (exitCode, exitStatus) {
                var st = root.imageStates[clipId]
                if (st) {
                    if (exitCode === 0) {
                        st.state = "ready"
                        st.source = root.previewPath(clipId)
                    } else {
                        st.state = "error"
                        st.source = ""
                    }
                    root.imageStatesChanged()
                }
                destroy()
            }
        }
    }

    property Process lister: Process {
        command: ["cliphist", "list"]
        // No CLIPHIST_DB_PATH override — cliphist's default ~/.cache/cliphist/db
        // is persistent across reboots, unlike the volatile XDG_RUNTIME_DIR.

        stdout: StdioCollector {
            id: listOut
            onStreamFinished: {
                var lines = ("" + listOut.text).split("\n")
                var parsed = []
                for (var i = 0; i < lines.length && parsed.length < root.maxEntries; i++) {
                    var line = lines[i]
                    var tab = line.indexOf("\t")
                    if (tab < 1) continue
                    var idText = line.substring(0, tab)
                    if (!/^[0-9]+$/.test(idText)) continue
                    var raw = line.substring(tab + 1)
                    var imgMeta = root.parseImageMeta(raw)
                    if (imgMeta) {
                        parsed.push({
                            "id": idText,
                            "kind": "image",
                            "preview": imgMeta.metadata,
                            "format": imgMeta.format,
                            "width": imgMeta.width,
                            "height": imgMeta.height,
                            "imageSource": "",
                            "previewState": "none"
                        })
                    } else {
                        parsed.push({
                            "id": idText,
                            "kind": "text",
                            "preview": root.previewOf(raw),
                            "imageSource": "",
                            "previewState": "none"
                        })
                    }
                }
                root.entries = parsed
                root.loadError = false
                root.loading = false
            }
        }

        onExited: function (exitCode, exitStatus) {
            if (exitCode !== 0 && root.entries.length === 0) {
                root.loadError = true
                root.error = "cliphist list failed"
                root.loading = false
            }
        }
    }
}
