// StagingEngine.qml — the staging engine: resolves the active rice's theme
// tokens to per-slot QML.
//
// The strillix lane's build (modules/dendrites/strillix/runtime.nix) carries
// every committed rice's authored widget slots into $out/qml/rices/<name>/
// <slot>.qml, plus a generated rices/manifest.json — an OWNER MAP recording
// which rice's manifest entry provides each slot, and which rice's directory
// the body actually lives under — e.g. `{ "sonata": { "calendar": { "owner":
// "sonata", "file": "calendar.qml" } } }`. A record's `owner` usually equals
// its own rice key, but need not: a rice that borrows a slot from another
// rice's shelf names that lender as the owner, so `{ "borrower": { "bar":
// { "owner": "lender" } } }` is a borrowed `bar`. Because provenance is
// recorded per-record, resolving a borrow is a direct lookup rather than a
// fallback-chain walk — and a borrowed body still receives the BORROWING
// rice's theme, since palette arrives as an injected prop (below) and is
// never read from disk by the widget. This singleton
// reads that manifest (hot-reloaded, so a rebuild's new manifest is picked up
// without restarting Quickshell) and answers the two questions a WidgetSlot
// needs: does <rice> dress <slot> (`has`), and where is its QML (`source`).
//
// Fixed injected-prop contract (CONTRACTS.md §5 containment): a loaded rice
// widget receives ONLY `theme` (ThemeState) and `bridge` (ShellBridge) —
// plus whatever slot-specific extras the anchor declares (e.g. notifications'
// `notification`) — never nix `config.*`. A rice widget is store-copied
// score, structurally incapable of reaching host/lane options through this
// surface.
//
// ── Declared widget-type registry (Phase 4) ─────────────────────────────────
// A second, independent data source: the same build (Phase 2) and
// `rice stage`/`preview` hot-sync (Phase 3) also carry each rice's
// `strillix.layout.widgets` declarations into a sibling
// rices/registry.json, shaped `{ "<rice>": { "<slot>": {…declaration…} } }`
// — which slots a rice registers as widget-TYPE declarations (kind/
// namespace/layer/shortcut/blur), a rarer, smaller set than manifest.json's
// "which slot bodies exist". `declaredWidgets(rice)` below answers "what did
// <rice> register" the way `has`/`source` answer "does <rice> dress <slot>"
// — a parallel accessor, not a replacement; `resolveRice`/body-loading are
// unchanged.

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    readonly property string manifestPath:
        (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/run/qml/rices/manifest.json"

    // The baseline-fallback floor (CONTRACTS.md §5): when the ACTIVE rice
    // doesn't dress a slot, resolution falls back to this rice before
    // falling back further to the anchor's own lane-side `fallback`
    // Component. Sonata is the shipped, guaranteed-present standard rice —
    // a fixed constant here, independent of `strillix.rice`'s own default
    // (`modules/nucleus/options.nix`, null) — so it's the correct floor to
    // catch every other rice's gaps.
    readonly property string baselineRice: "sonata"

    // { "<rice>": { "<slot>": { "owner": …, "file": … } }, … } — empty until
    // the first successful parse.
    property var manifest: ({})

    // Declared as a property (not a default-child) because QtObject has no
    // default property — the ThemeState.themeFile idiom.
    property FileView manifestFile: FileView {
        id: manifestFile
        path: root.manifestPath
        watchChanges: true
        onFileChanged: manifestFile.reload()
        onTextChanged: {
            try {
                root.manifest = JSON.parse(manifestFile.text())
            } catch (e) {
                console.warn("[aoide/stagingengine] Failed to parse rices/manifest.json:", e)
            }
        }
        Component.onCompleted: manifestFile.reload()
    }

    readonly property string registryPath:
        (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/run/qml/rices/registry.json"

    // { "<rice>": { "<slot>": {…declaration…} }, … } — empty until the first
    // successful parse.
    property var registry: ({})

    // Same named-property idiom as manifestFile above (QtObject has no
    // default property) — an independent FileView watching the sibling
    // registry.json, same directory/reload/parse shape as manifestFile.
    property FileView registryFile: FileView {
        id: registryFile
        path: root.registryPath
        watchChanges: true
        onFileChanged: registryFile.reload()
        onTextChanged: {
            try {
                root.registry = JSON.parse(registryFile.text())
            } catch (e) {
                console.warn("[aoide/stagingengine] Failed to parse rices/registry.json:", e)
            }
        }
        Component.onCompleted: registryFile.reload()
    }

    // <rice>'s declared widget-type registrations — `{}` when the rice has
    // none, or the file hasn't loaded yet. Never throws: an unknown rice or
    // a not-yet-parsed registry both cleanly answer `{}`, same
    // bounds-checked posture as `has` below.
    function declaredWidgets(rice) {
        if (!rice || !root.registry || !root.registry[rice]) return {}
        return root.registry[rice]
    }

    // Does <rice>'s manifest entry provide <slot>? Bounds-checked: an
    // unknown rice or a rice with no entry for <slot> both cleanly answer
    // false. The manifest is an OWNER MAP (`{ "<rice>": { "<slot>": {
    // "owner": …, "file": … } } }`), so this is a direct object lookup, not
    // the list scan (`indexOf`) the pre-W3b shape needed.
    function has(rice, slot) {
        return !!(rice && root.manifest && root.manifest[rice] && root.manifest[rice][slot])
    }

    // Resolved URL for <rice>'s <slot> widget — only meaningful when has()
    // is true; callers gate on that first. Reads the slot's OWNER out of
    // <rice>'s manifest entry — usually <rice> itself, but a borrow names a
    // different rice (a borrowed `bar` is owned by its lender) — to find
    // which rice's copied rices/<owner>/ directory the body physically
    // lives under, and the entry's `file` for the body's basename there.
    function source(rice, slot) {
        var entry = root.manifest[rice][slot]
        return Qt.resolvedUrl("rices/" + entry.owner + "/" + entry.file)
    }

    // ── Baseline-fallback resolution (CONTRACTS.md §5) ──────────────────────
    // Resolve <slot> to the rice that actually authors it: <rice> itself if
    // it dresses the slot, else the baseline (sonata) if IT dresses the
    // slot, else "" (no rice provides it — the caller falls back further to
    // its own lane-side `fallback` Component, or renders nothing).
    function resolveRice(rice, slot) {
        if (root.has(rice, slot)) return rice
        if (root.has(root.baselineRice, slot)) return root.baselineRice
        return ""
    }
}
