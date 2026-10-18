//@ pragma UseQApplication
// shell.qml — Aoide Quickshell root.
//
// QApplication mode (the pragma above, 2026-08-23): required so
// QsMenuAnchor.open() can show platform menus — the bar's tray popout opens
// SNI items' dbusmenus (nm-applet first); quickshell hard-errors the call in
// its default QGuiApplication mode. Costs a restart to take effect; no other
// behavior difference observed on this rig.
//
// Entry point for the Quickshell session. Instantiates the infrastructure
// (theme, bridge, staging engine, IPC, clipboard, ledger, RiceSurfaces)
// unconditionally and each component anchor only when its slot is in
// `components.json` (the component gate below), and wires the shared theme
// loader (ThemeState) so every widget hot-reloads from rice/stage/theme.json
// when it changes.
//
// Communication discipline (CONTRACTS.md / entities/Quickshell):
//   - Reads state files from rice/stage/ (ThemeState watches theme.json).
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
import Quickshell.Hyprland

ShellRoot {
    id: shellRoot

    // A Variants delegate does not resolve this file's ids in its binding
    // scope — `theme: theme` inside one silently binds to undefined rather
    // than failing loudly, and the first symptom is a TypeError deep inside the
    // delegate's own child (observed live: "Cannot read property 'paletteBg' of
    // undefined" from AoideWallpaper). This handle is the delegate-safe way to
    // reach the shared ThemeState: it is resolved HERE, in root scope, and
    // read through the root id from inside the delegate.
    //
    // The hazard is broader than that one binding: a delegate child that
    // DECLARES a property under the same name as a root-scope id gets no id at
    // all there. `bridge: bridge` / `stagingEngine: stagingEngine` on a
    // WidgetSlot inside a delegate resolve to the anchor's own not-yet-set
    // property and settle at undefined — silently, exactly like the theme
    // case above, with the loud symptom landing one layer down ("Cannot read
    // property 'riceName' of undefined" from WidgetSlot's own resolver). Ids
    // nothing collides with (`shared`, `clipboard`, `ledger`) still
    // resolve. Every root singleton a delegate reaches gets a handle for that
    // reason.
    readonly property var themeRef: theme
    readonly property var bridgeRef: bridge
    readonly property var stagingRef: stagingEngine

    // ── The component gate (slots.md, CONTRACTS.md §4 components.json) ────
    // Which component slots this host filled, read once from the
    // `components.json` the build writes beside this file. A component anchor
    // below is instantiated only when `has(<slot>)`; an absent or unreadable
    // file is the empty set, so nothing is instantiated (fail closed). The
    // file is build data, rewritten only by a rebuild, so it is not watched.
    property FileView componentsFile: FileView {
        path: Quickshell.shellPath("components.json")
        blockLoading: true
        printErrors: false
        onLoadFailed: console.warn("[aoide/shell] components.json unreadable: no component is instantiated")
    }
    readonly property var components: {
        try {
            var v = JSON.parse(componentsFile.text())
            return (v && typeof v === "object" && !Array.isArray(v)) ? v : ({})
        } catch (e) {
            return ({})
        }
    }
    function has(slot) {
        return Object.prototype.hasOwnProperty.call(shellRoot.components, slot)
    }

    // A gated anchor's live body, or null while its component is not
    // instantiated: what the bar and the wallpaper picker are handed.
    readonly property var powermenuItem: powermenuGate.item ? powermenuGate.item.item : null
    readonly property var dockItem: dockGate.item ? dockGate.item.body : null
    readonly property var launcherItem: launcherGate.item ? launcherGate.item.item : null
    readonly property var calendarItem: calendarGate.item ? calendarGate.item.body : null
    readonly property var switcherItem: switcherGate.item ? switcherGate.item.body : null

    // What the `component` IPC target runs a verb on, by slot. Components never
    // reach each other's items: a caller asks the bridge (`bridge.component`),
    // and the bridge arrives here as `act`.
    function componentItem(slot) {
        switch (slot) {
        case "powermenu": return shellRoot.powermenuItem
        case "dock": return shellRoot.dockItem
        case "launcher": return shellRoot.launcherItem
        case "calendar": return shellRoot.calendarItem
        case "switcher": return shellRoot.switcherItem
        default: return null
        }
    }
    readonly property var componentVerbs: ["toggle", "show", "hide", "openTab", "openPictures"]
    // The launcher's own Desktop Pictures window, as a fact: the wallpaper
    // picker asks the bridge for it only when the launcher can answer.
    readonly property bool launcherHasPictures: !!shellRoot.launcherItem && typeof shellRoot.launcherItem.openPictures === "function"

    // ── Shared singletons (one instance for the whole session) ─────────────
    ThemeState { id: theme }
    ShellBridge { id: bridge }
    // `strillix reload`'s IPC target (crates/strillix-rice/src/ipc.rs) — no
    // properties, wired for its side effect (Quickshell.reload) alone.
    AoideIpc { id: ipc }
    // The `component` IPC target: `strillix component <slot> <verb> [arg]` through
    // the bridge. It runs one closed verb on the slot's live item and answers
    // `true` when it ran, `false` when the item is not loaded or lacks the verb.
    // A void answer could not tell those apart, so this one returns a value.
    IpcHandler {
        target: "component"
        function act(slot: string, verb: string, arg: string): bool {
            var item = shellRoot.componentItem(slot)
            if (!item || shellRoot.componentVerbs.indexOf(verb) < 0 || typeof item[verb] !== "function")
                return false
            if (arg === "") item[verb](); else item[verb](arg)
            return true
        }
    }
    // The staging engine (CONTRACTS.md §5) — reads the manifest
    // the strillix lane's build carries into $AOIDE_ROOT/run/qml/rices/, resolves
    // WidgetSlot's "does the active rice dress this slot" / "where's its QML".
    StagingEngine { id: stagingEngine }

    // ── Shared session state (floating gadgets + DAG trace link) ───────────
    // A plain QtObject passed by property, exactly like theme/bridge — the
    // cleanest quickshell idiom for cross-widget state that needs no file
    // watch of its own. Holds: (a) `floatingModel`, the ListModel of gadgets
    // torn off onto DesktopGadgets ({ kind, gx, gy }); and (b) `tracedSessionId`,
    // the hover-trace link — TerminalManagerGadget writes it on row hover,
    // DagGraphGadget highlights the node whose id matches. Session-scoped, no
    // persistence (v1). QtObject has no default property, so the ListModel is a
    // named property (the ThemeState.themeFile idiom).
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

        // The calendar's anchor (screen coordinates, measured by the bar's clock
        // before it asks the bridge to toggle the calendar) and its open state
        // (published by the calendar surface, read by the clock's lit state).
        property point calendarAnchor: Qt.point(0, 0)
        property bool calendarOpen: false
        // The dock's state, published by the dock surface for the bar's lit cells.
        property bool dockShown: false
        property var dockScreen: null

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
    // Solid palette-bg fallback until a wallpaper is staged; sits beneath every
    // window. Empty input mask → never intercepts desktop clicks.
    //
    // ONE LAYER SURFACE PER OUTPUT. A bare PanelWindow binds to a single
    // screen, so on a multi-head rig the wallpaper painted exactly one monitor and
    // every other output fell through to bare compositor ground — invisible on
    // a single-panel laptop, immediate on a two-monitor desk (osaka, 2026-08-30).
    // Variants is quickshell's idiom for this: its default property IS the
    // delegate, so the PanelWindow below is instantiated once per entry in
    // Quickshell.screens, and `screen: modelData` pins each instance to its own
    // output. The delegate needs `required property var modelData` for Variants
    // to inject the row at all.
    //
    // Every instance draws the SAME wallpaper — AoideWallpaper resolves one path
    // (AOIDE_WALLPAPER, or stage/wallpaper.json when staged), and the wallpaper note is
    // a single value on the rice. Per-output DIFFERENT wallpapers would need a
    // per-screen wallpaper map in the theme schema; deliberately not invented here.
    Variants {
        model: shellRoot.has("wallpaper") ? Quickshell.screens : []

        PanelWindow {
            id: wallpaperWin
            required property var modelData
            screen: modelData

            anchors { top: true; bottom: true; left: true; right: true }
            // -1 = ignore other surfaces' exclusive zones, so the Background layer
            // spans the WHOLE output (0,0 → full) and reaches UNDER the bar's 36px
            // reservation. With 0 the bar's exclusiveZone evicts the wallpaper to
            // y=36 and the wallpaper never renders behind the translucent strip.
            exclusiveZone: -1
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Background
            WlrLayershell.namespace: "aoide-wallpaper"
            mask: Region { width: 0; height: 0 }

            AoideWallpaper {
                anchors.fill: parent
                // Root-qualified on purpose — see shellRoot.themeRef above.
                theme: shellRoot.themeRef
                // The `wallpaper` slot's own needs (slots.md): the active rice's
                // board is resolved through the staging engine, and a board that
                // reads the machine gets the same bridge every other widget does.
                bridge: shellRoot.bridgeRef
                stagingEngine: shellRoot.stagingRef
            }
        }
    }

    // ── Bar (top edge, exclusive — reserves its height) ────────────────────
    // Whole-content slot (CONTRACTS.md §5, slots.md): the PanelWindow itself
    // — layer, namespace, exclusive-zone reservation — stays lane-owned;
    // only its CONTENT is the per-rice `bar` widget, loaded through
    // WidgetSlot like calendar/notifications. The rice fully owns its own
    // footprint now: implicitHeight/exclusiveZone read back from the loaded
    // item's own implicitHeight rather than a lane-pinned constant.
    //
    // ONE BAR PER OUTPUT, same as the wallpaper above: a singleton bound to
    // one dead screen stayed dead after an output blip, while a Variants
    // delegate is rebuilt from the new Quickshell.screens list and re-homes
    // itself. `screen: modelData` pins each instance to its own output.
    Variants {
        model: shellRoot.has("bar") ? Quickshell.screens : []

        PanelWindow {
            id: barWin
            required property var modelData
            screen: modelData

            anchors { top: true; left: true; right: true }
            implicitHeight: barSlot.implicitHeight
            exclusiveZone: barSlot.implicitHeight
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Top
            // Distinct namespace → target of the compositor lane's glass
            // layerrule (blur behind the translucent barBg — dxflake's
            // "namespace waybar" posture, aoide-native name).
            WlrLayershell.namespace: "aoide-bar"

            WidgetSlot {
                id: barSlot
                anchors.fill: parent
                // Root-qualified singletons: WidgetSlot declares properties of
                // these very names, which is what blinds a delegate's bare id
                // reference — see shellRoot's handle block above.
                theme: shellRoot.themeRef
                bridge: shellRoot.bridgeRef
                stagingEngine: shellRoot.stagingRef
                slot: "bar"
                // stagingEngine is ALSO threaded as an extra (beyond the
                // anchor's own required stagingEngine above) because the loaded
                // bar widget needs its own handle to resolve its embedded
                // ricemode WidgetSlot — two separate reads of the same
                // singleton, not a conflict. The bar takes no other component's
                // item: it asks the bridge (`bridge.component(...)`).
                extraProps: ({
                    shared: shared,
                    stagingEngine: shellRoot.stagingRef
                })
            }
        }
    }

    // ── Center-left dock (surface: "dock") ──────────────────────────────────
    // The temple's home: five self-framed stele gadgets (Conductor, Usage,
    // Terminals, Meters, Power) stacked into one summoned column pinned to the
    // LEFT edge, vertically centred. Sonata's own `widgets/dock.qml` owns the
    // whole surface — its own PanelWindow, WlrLayershell namespace and
    // keyboard focus — the same `SurfaceSlot` contract powermenu/launcher/
    // notifications already carry (slots.md); it replaced the lane's own
    // `AoidePanel.qml`, retired once this slot anchor landed. The toggle
    // keybind stays a lane contract on purpose (so a rice's dock body can
    // never forget to bind it): `aoide:dock` (SUPER+G) is a Hyprland global
    // shortcut registered here, dispatching into whatever `.item` the slot
    // resolves — the same pattern RiceSurfaces uses for a declared widget's
    // own shortcut.
    //
    // Gated like every component anchor: the slot and its shortcut live in one
    // Scope the gate creates only when `dock` is selected. Inside a gate the
    // root ids `theme`/`bridge`/`stagingEngine` are shadowed by the anchor's
    // own properties of those names (the delegate hazard above), so the
    // anchor reads the root handles.
    LazyLoader {
        id: dockGate
        active: shellRoot.has("dock")

        Scope {
            readonly property var body: dockSlot.item

            SurfaceSlot {
                id: dockSlot
                slot: "dock"
                theme: shellRoot.themeRef
                bridge: shellRoot.bridgeRef
                stagingEngine: shellRoot.stagingRef
                // SurfaceSlot's fixed contract forwards only theme+bridge to the
                // loaded body; `shared` and `stagingEngine` are threaded as extras
                // because dock.qml's own embedded WidgetSlot gadgets (conductor,
                // terminals, …) need both to resolve and render their own column —
                // the same two-reads-of-one-singleton idiom barSlot uses above.
                extraProps: ({ shared: shared, stagingEngine: shellRoot.stagingRef })
            }
            GlobalShortcut {
                appid: "aoide"
                name: "dock"
                description: "Summon the Aoide center-left codex dock"
                onPressed: {
                    var item = dockSlot.item
                    if (item && item.toggle) item.toggle()
                }
            }
        }
    }

    // ── Overlay surfaces — each owns its own PanelWindow internally, wired
    // to theme/bridge only. Hidden/dormant until summoned/triggered, and
    // each behind its component's gate (`launcher`, `powermenu`, `wallpaper`
    // for the picker, `switcher`, `notifications` for notifications).
    //   - launcher (SurfaceSlot) : SUPER+Space launcher (bridge-toggled),
    //                              rices/sonata/widgets/launcher.qml
    //   - powermenu (SurfaceSlot): the powermenu (bar clef 𝄞 → powermenu.toggle()),
    //                              rices/sonata/widgets/powermenu.qml
    //   - AoideWallpaperPicker   : aoide:wallpaper — the launcher's openPictures(),
    //                              else the plain wallpaper picker (global-shortcut)
    //   - switcher (SurfaceSlot) : aoide:switcher window switcher, the rice's
    //                              widgets/switcher.qml or the plain AoideSwitcher
    //   - notifications (SurfaceSlot)   : the notification popup,
    //                              rices/sonata/widgets/notifications.qml
    // (No AoideNotifications anymore — dunst owns org.freedesktop.Notifications
    // as the DAEMON, but draws nothing. It feeds `strx notifications push`, the
    // shellbridge files each notification into state/stage/notifications.json, and
    // the `notifications` slot below draws the popup — with the images and progress bars
    // inside the frame, and real hit-tested approve/deny buttons on a
    // permission summons, none of which dunst could draw itself. The dock's
    // `notifications-center` slot reads the same file as a ledger.)
    AoideClipboard { id: clipboard }
    // The Grimoire's usage ledger — stays in the lane (a data seam, not
    // chrome, CONTRACTS.md §4), injected into the launcher slot as an extra.
    GrimoireLedger { id: ledger }
    // powermenu/launcher: window-owning slots (SurfaceSlot, not WidgetSlot —
    // each roots its own PanelWindow). `.item` is the loaded rice widget's
    // live handle, resolved through the baseline chain (sonata is the
    // floor, so `.item` is only null if sonata itself somehow lacks the
    // file — shouldn't happen once these slots are populated).
    LazyLoader {
        id: powermenuGate
        active: shellRoot.has("powermenu")

        SurfaceSlot {
            slot: "powermenu"
            theme: shellRoot.themeRef
            bridge: shellRoot.bridgeRef
            stagingEngine: shellRoot.stagingRef
        }
    }
    LazyLoader {
        id: launcherGate
        active: shellRoot.has("launcher")

        SurfaceSlot {
            slot: "launcher"
            theme: shellRoot.themeRef
            bridge: shellRoot.bridgeRef
            stagingEngine: shellRoot.stagingRef
            extraProps: ({ clipboard: clipboard, ledger: ledger })
        }
    }
    // notifications: the notification popup (see the overlay-surfaces note above),
    // the `notifications` component — the slot's PanelWindow watches
    // state/stage/notifications.json and stays dormant while the ledger is empty.
    // The calendar: a window-owning surface, asked for by the bar's clock
    // through the bridge. Its body anchors under the clock and publishes its
    // open state on `shared`.
    LazyLoader {
        id: calendarGate
        active: shellRoot.has("calendar")

        Scope {
            readonly property var body: calendarSlot.item

            SurfaceSlot {
                id: calendarSlot
                slot: "calendar"
                theme: shellRoot.themeRef
                bridge: shellRoot.bridgeRef
                stagingEngine: shellRoot.stagingRef
                extraProps: ({ shared: shared })
            }
            GlobalShortcut {
                appid: "aoide"
                name: "calendar"
                description: "Summon the Aoide calendar"
                onPressed: {
                    var item = calendarSlot.item
                    if (item && item.toggle) item.toggle()
                }
            }
        }
    }
    LazyLoader {
        active: shellRoot.has("notifications")

        SurfaceSlot {
            slot: "notifications"
            theme: shellRoot.themeRef
            bridge: shellRoot.bridgeRef
            stagingEngine: shellRoot.stagingRef
        }
    }
    // The declared widget-type registry's runtime half (CONTRACTS.md §5;
    // aoide.layout.widgets, registry.json): hosts one SurfaceSlot per
    // surface-kind entry the active rice declares. Empty registry (sonata
    // today) → no-op, same as powermenu/launcher above but data-driven
    // instead of a fixed slot name.
    RiceSurfaces {
        theme: theme
        bridge: bridge
        stagingEngine: stagingEngine
    }
    // aoide:wallpaper: the launcher's own Desktop Pictures when its body has
    // `openPictures()` (null while `launcher` is not selected), else the plain
    // picker (AoideWallpaperPicker.qml). Part of the `wallpaper` component.
    LazyLoader {
        active: shellRoot.has("wallpaper")

        AoideWallpaperPicker {
            theme: shellRoot.themeRef
            bridge: shellRoot.bridgeRef
            launcherHasPictures: shellRoot.launcherHasPictures
        }
    }

    // The session lock (LockHost.qml): one WlSessionLock + PamContext, the
    // active rice's own `lockscreen.qml` face on every screen, and the floor
    // field under it. `strillix lock` is its only entry (`ipc call lock lock`),
    // so a host without the `lock` component has no `lock` IPC target.
    LazyLoader {
        active: shellRoot.has("lock")

        LockHost {
            theme: shellRoot.themeRef
            stagingEngine: shellRoot.stagingRef
        }
    }

    // aoide:switcher: the one registration; it toggles whichever body the
    // `switcher` slot holds — the active rice's widgets/switcher.qml, else
    // sonata's, else the plain AoideSwitcher below (the floor). Not bound
    // here: the venue binds a combo to it.
    LazyLoader {
        id: switcherGate
        active: shellRoot.has("switcher")

        Scope {
            readonly property var body: switcherSlot.item

            SurfaceSlot {
                id: switcherSlot
                slot: "switcher"
                theme: shellRoot.themeRef
                bridge: shellRoot.bridgeRef
                stagingEngine: shellRoot.stagingRef
                fallback: Component { AoideSwitcher {} }
            }
            GlobalShortcut {
                appid: "aoide"
                name: "switcher"
                description: "Summon the Aoide window switcher"
                onPressed: {
                    var item = switcherSlot.item
                    if (item && item.toggle) item.toggle()
                }
            }
        }
    }
}
