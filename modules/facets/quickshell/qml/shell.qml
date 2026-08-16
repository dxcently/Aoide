// shell.qml — Aoide Quickshell root.
//
// Entry point for the Quickshell session. Instantiates all surface widgets
// and wires the shared note loader (LiveryState) so every widget hot-reloads
// from song/stage/livery.json when it changes.
//
// Communication discipline (CONTRACTS.md / entities/Quickshell):
//   - Reads state files from song/stage/ (LiveryState watches livery.json).
//   - Issues commands to shellbridge via unix socket (ShellBridge singleton).
//   - Never speaks MCP or any agent protocol.
//
// Layer-shell wrappers (v1): the surface widgets are self-contained Items;
// shell.qml wraps the ones that own screen real estate in PanelWindows so
// they register real Wayland layer surfaces (the wrapper is the "shared
// layer-shell typing" the widget headers anticipate). Each widget still
// fills `parent` (the window content), so the files stay swappable Items.

import QtQuick
import QtQml.Models
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    // ── Shared singletons (one instance for the whole session) ─────────────
    LiveryState { id: notes }
    ShellBridge { id: bridge }
    // `aoide quickshell reload`'s IPC target (crates/song/src/ipc.rs) — no
    // properties, wired for its side effect (Quickshell.reload) alone.
    AoideIpc { id: ipc }
    // The staging engine (CONTRACTS.md §5) — reads the manifest
    // the quickshell facet's build carries into ~/Aoide/run/qml/songs/, resolves
    // WidgetSlot's "does the active song dress this slot" / "where's its QML".
    StagingEngine { id: stagingEngine }

    // ── Shared session state (floating gadgets + DAG trace link) ───────────
    // A plain QtObject passed by property, exactly like notes/bridge — the
    // cleanest quickshell idiom for cross-widget state that needs no file
    // watch of its own. Holds: (a) `floatingModel`, the ListModel of gadgets
    // torn off onto DesktopGadgets ({ kind, gx, gy }); and (b) `tracedSessionId`,
    // the hover-trace link — TerminalManagerGadget writes it on row hover,
    // DagGraphGadget highlights the node whose id matches. Session-scoped, no
    // persistence (v1). QtObject has no default property, so the ListModel is a
    // named property (the LiveryState.noteFile idiom).
    QtObject {
        id: shared

        property ListModel floatingModel: ListModel {}
        property string tracedSessionId: ""

        // Hover-preview bridge (concepts/Terminal-Commander): the roster gadgets
        // write the hovered terminal row's Hyprland workspace id here; the bar's
        // WorkspaceRow reads it and paints a distinct PREVIEW highlight on that
        // workspace glyph. -1 is the sentinel for "nothing hovered" (no real
        // workspace carries id -1). Pure QML data link — no hyprctl dispatch.
        property int hoveredWorkspace: -1
        // The IDENTITY of the hovered row (its sessionId), tracked alongside the
        // workspace int so a row DELEGATE that gets torn down + recreated by a
        // roster refresh (sessions.json / hyprctl re-read) can re-assert the
        // highlight from its own id instead of losing it — the fix for the
        // "highlight flickers then vanishes while still hovering" bug. "" = none.
        property string hoveredSessionId: ""

        // Cascading seed position so successive tear-offs don't stack exactly.
        function floatGadget(kind) {
            var base = 360
            var step = 34
            var n = floatingModel.count
            floatingModel.append({
                "kind": kind,
                "gx": base + (n % 4) * step,
                "gy": 80 + (n % 4) * step
            })
        }
        function unfloatGadget(index) {
            if (index >= 0 && index < floatingModel.count)
                floatingModel.remove(index)
        }
    }

    // ── Wallpaper (background layer, full screen, click-through) ───────────
    // Solid palette-bg fallback until a cover is staged; sits beneath every
    // window. Empty input mask → never intercepts desktop clicks.
    PanelWindow {
        id: wallpaperWin
        anchors { top: true; bottom: true; left: true; right: true }
        // -1 = ignore other surfaces' exclusive zones, so the Background layer
        // spans the WHOLE output (0,0 → full) and reaches UNDER the bar's 36px
        // reservation. With 0 the bar's exclusiveZone evicts the wallpaper to
        // y=36 and the cover never renders behind the translucent strip.
        exclusiveZone: -1
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Background
        WlrLayershell.namespace: "aoide-wallpaper"
        mask: Region { width: 0; height: 0 }

        AoideWallpaper {
            anchors.fill: parent
            notes: notes
        }
    }

    // ── Bar (top edge, exclusive — reserves its height) ────────────────────
    // Whole-content slot (CONTRACTS.md §5, slots.md): the PanelWindow itself
    // — layer, namespace, exclusive-zone reservation — stays facet-owned;
    // only its CONTENT is the per-song `bar` widget, loaded through
    // WidgetSlot like calendar/notifications. The song fully owns its own
    // footprint now: implicitHeight/exclusiveZone read back from the loaded
    // item's own implicitHeight rather than a facet-pinned constant.
    PanelWindow {
        id: barWin
        anchors { top: true; left: true; right: true }
        implicitHeight: barSlot.implicitHeight
        exclusiveZone: barSlot.implicitHeight
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        // Distinct namespace → target of the compositor facet's glass
        // layerrule (blur behind the translucent barBg — dxflake's
        // "namespace waybar" posture, aoide-native name).
        WlrLayershell.namespace: "aoide-bar"

        WidgetSlot {
            id: barSlot
            anchors.fill: parent
            notes: notes
            bridge: bridge
            stagingEngine: stagingEngine
            slot: "bar"
            // stagingEngine is ALSO threaded as an extra (beyond the
            // anchor's own required stagingEngine above) because the loaded
            // bar widget needs its own handle to resolve its embedded
            // calendar WidgetSlot — two separate reads of the same
            // singleton, not a conflict. `powermenu` is the powermenu
            // slot's live item (declared below with the overlay surfaces —
            // QML resolves id references regardless of order, the
            // clipboard→launcher precedent).
            extraProps: ({
                shared: shared,
                powermenu: powermenuSlot.item,
                stagingEngine: stagingEngine
            })
        }
    }

    // ── Center-left dock (surface: "dock") ─────────────────────────────────
    // The temple's home: four self-framed stele gadgets (Conductor, Terminals,
    // Meters, Power) stacked into one summoned column pinned to the LEFT edge,
    // vertically centred. Owns its own PanelWindow + WlrLayershell + toggle
    // GlobalShortcut (SUPER+P → aoide:dock) internally — shell.qml just hands it
    // the shared singletons. Replaces the old AoideAgentWidgets hot-edge drawer.
    AoidePanel {
        notes: notes
        bridge: bridge
        shared: shared
    }

    // ── Overlay surfaces — each owns its own PanelWindow internally, wired
    // to notes/bridge only. Hidden/dormant until summoned/triggered.
    //   - launcher (SurfaceSlot) : SUPER+Space launcher (bridge-toggled),
    //                              song/songbook/sonata/widgets/launcher.qml
    //   - powermenu (SurfaceSlot): the powermenu (bar clef 𝄞 → powermenu.toggle()),
    //                              song/songbook/sonata/widgets/powermenu.qml
    //   - AoideWallpaperPicker   : SUPER+W wallpaper switcher (global-shortcut)
    //   - AoideNotifications     : org.freedesktop.Notifications popup stack,
    //                              bottom-right (live whenever a notification exists)
    AoideClipboard { id: clipboard }
    // The Grimoire's usage ledger — stays in the facet (a data seam, not
    // chrome, CONTRACTS.md §4), injected into the launcher slot as an extra.
    GrimoireLedger { id: ledger }
    // powermenu/launcher: window-owning slots (SurfaceSlot, not WidgetSlot —
    // each roots its own PanelWindow). `.item` is the loaded song widget's
    // live handle, resolved through the baseline chain (sonata is the
    // floor, so `.item` is only null if sonata itself somehow lacks the
    // file — shouldn't happen once these slots are populated).
    SurfaceSlot {
        id: powermenuSlot
        slot: "powermenu"
        notes: notes
        bridge: bridge
        stagingEngine: stagingEngine
    }
    SurfaceSlot {
        id: launcherSlot
        slot: "launcher"
        notes: notes
        bridge: bridge
        stagingEngine: stagingEngine
        extraProps: ({ clipboard: clipboard, ledger: ledger })
    }
    AoideWallpaperPicker { notes: notes }
    AoideNotifications { notes: notes; bridge: bridge; stagingEngine: stagingEngine }
}
