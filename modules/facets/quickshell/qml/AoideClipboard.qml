// AoideClipboard.qml — the cliphist clipboard-backend for the Grimoire.
//
// Entries are display-only metadata. Copying accepts only a validated numeric
// cliphist identifier; clipboard contents never become a command. Image
// thumbnails are decoded through the fixed-path aoide-clipboard-preview helper
// (never by a shell command built from user data).

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    property bool loading: false
    property bool loadError: false
    property string error: ""
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

    // Start decoding a preview image for one validated numeric id. Returns
    // the fixed cache file URL on success, or an empty string on failure.
    // Never constructs a shell command from clipboard data — the helper only
    // accepts a numeric id.
    function requestPreview(id) {
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

    // The runtime cache path atomically written by aoide-clipboard-preview.
    function previewPath(id) {
        var cache = Quickshell.env("XDG_CACHE_HOME") || (Quickshell.env("HOME") + "/.cache")
        return "file://" + cache + "/aoide-clipboard/previews/" + id
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
        // Deliberately pass only the validated identifier. Never pass preview
        // text, and never construct a shell command from clipboard contents.
        Quickshell.execDetached(["aoide-clipboard-copy", id])
        return true
    }

    // Factory for one-shot preview decoders — each instance decodes exactly
    // one numeric id and self-destructs.
    property Component previewFactory: Component {
        Process {
            property string clipId: ""
            command: ["aoide-clipboard-preview", clipId]
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
