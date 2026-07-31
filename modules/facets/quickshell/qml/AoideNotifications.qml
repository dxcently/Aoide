// AoideNotifications.qml — native notification popup stack (surface: "notifications").
//
// Quickshell implements org.freedesktop.Notifications natively via
// Quickshell.Services.Notifications' NotificationServer — no mako, no
// swaync. This widget owns the whole notification surface: claims the D-Bus
// name, tracks incoming notifications, and renders them as a popup stack
// pinned to the BOTTOM-right corner (khoa, 2026-07-30 — moved off the
// original top-right placement).
//
// This is core SHARED shell chrome (like the bar/launcher/wallpaper) — wired
// into shell.qml exactly like AoideLauncher/AoideWallpaperPicker. The per-card
// BODY is now a per-song flavor-widget slot (CONTRACTS.md §5, "notifications"):
// each card renders through WidgetSlot, which loads the active song's own
// `widgets/notifications.qml` when it authored one, else falls back to the
// shared NotificationCard below — this pass ships no song-authored
// notifications.qml, so every card renders via that shared fallback (the
// override anchor is wired now; a future song dropping the file needs zero
// further code change here).
//
// ── API grounding (real, not guessed) ───────────────────────────────────────
// Quickshell ships this module as a compiled plugin; its QML surface is
// generated (qmltyperegistrar) into a .qmltypes file that was read in full
// before writing this file:
//   /nix/store/*-quickshell-wrapped-0.3.0/lib/qt-6/qml/Quickshell/Services/
//     Notifications/quickshell-service-notifications.qmltypes
// Confirmed from that file (NOT assumed):
//   - NotificationServer exposes `trackedNotifications` (an ObjectModel) —
//     NOT `.notifications`. The deleted skeleton (git 5924c89~1) bound a
//     Repeater to `notifServer.notifications`, a property that does not
//     exist on this type; that model would have stayed permanently empty
//     even if the rest of the file had been correct.
//   - `Notification.tracked` is a SETTABLE bool: a notification only enters
//     `trackedNotifications` once something sets `tracked = true`. The old
//     skeleton's `onNotification` handler was empty — it never did this — a
//     second, independent reason nothing would ever have rendered.
//   - `Notification.expireTimeout` is milliseconds; per the freedesktop spec
//     (which Quickshell follows here), -1 means "sender left it to us", 0
//     means "never auto-expire".
//   - `Notification.expire()` / `.dismiss()`, `NotificationAction.invoke()`,
//     the `closed(reason)` signal, and the `NotificationUrgency` /
//     `NotificationCloseReason` singletons are all real, confirmed methods —
//     see NotificationCard.qml, which is where they're actually used.
//
// Chrome/colors: notes-only, radius 0 throughout — see NotificationCard.qml
// for the per-card Greek-grammar treatment. This file only owns the server
// and the stack's positioning.

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Services.Notifications

PanelWindow {
    id: root

    required property var notes
    required property var bridge
    required property var stagingEngine

    // Shared fallback when the active song hasn't authored a "notifications"
    // widgets/notifications.qml — the existing Tuscan-stele card, unchanged.
    Component {
        id: notifCardComp
        NotificationCard {}
    }

    // ── The D-Bus notification server ───────────────────────────────────────
    // Capability flags tell senders what we actually render: plain body text
    // (no markup/hyperlinks — a v1 popup, not a rich renderer), actions
    // (rendered as buttons — NotificationCard), no inline reply, no
    // persistence beyond the process's own lifetime. `keepOnReload` survives
    // a `qs -c reload` (this shell hot-reloads QML often during dev).
    //
    // `imageSupported: false` — per server.cpp/notification.cpp, this flag
    // only controls whether "icon-static" is advertised in GetCapabilities'
    // response; `Notification.appIcon` is populated from the `Notify` call's
    // own arguments unconditionally, regardless of this flag. Since this card
    // renders no icons or images anywhere (see NotificationCard.qml's "NO
    // ICONS" note), advertising support here would just invite
    // capability-aware senders to attach image-data hints that get silently
    // dropped. `bodyImagesSupported` stays false for the same reason: inline
    // body markup images genuinely aren't rendered either.
    NotificationServer {
        id: notifServer
        keepOnReload: true
        bodySupported: true
        bodyMarkupSupported: false
        bodyHyperlinksSupported: false
        bodyImagesSupported: false
        actionsSupported: true
        actionIconsSupported: false
        imageSupported: false
        inlineReplySupported: false
        persistenceSupported: false

        // A notification is only rendered once it's TRACKED (see grounding
        // note above) — every incoming notification is tracked; NotificationCard
        // + its own timer/closed-handling decide when it untracks itself again.
        onNotification: (notification) => { notification.tracked = true }
    }

    // Oldest-first stack (natural arrival order off `trackedNotifications` —
    // a Quickshell ObjectModel, a real QAbstractListModel per model.hpp/
    // model.cpp: insertObject()/removeObject() drive proper
    // beginInsertRows/beginRemoveRows, and its one role, `modelData`, resolves
    // straight to the Notification object). The Repeater below binds this
    // model DIRECTLY — deliberately not a `.values.slice()` copy: a plain JS
    // array has no identity for a Repeater to diff against, so reassigning it
    // (which a `.slice()` does on every `valuesChanged`, i.e. every arrival OR
    // departure of ANY notification) tears down and recreates EVERY delegate,
    // restarting every OTHER card's auto-dismiss Timer and critical-urgency
    // pulse in the process. Binding straight to the ObjectModel lets the
    // Repeater diff on its real insert/remove signals instead, so only the
    // notification that actually arrived or left gets a delegate created or
    // destroyed — every other card's state (its Timer, its pulse animation)
    // survives untouched. `count` below is a plain reactive length read for
    // the window's `visible` binding — it's not what the Repeater uses.
    readonly property int count: notifServer.trackedNotifications && notifServer.trackedNotifications.values
        ? notifServer.trackedNotifications.values.length : 0

    // Deliberately NOT reversed: the Column below lays children out in THIS
    // order, and with the window bottom-anchored (below), the Column's LAST
    // child sits at the anchored, screen-fixed bottom edge — so keeping
    // newest-last here is what puts the newest notification AT the corner
    // (the natural toast-stack convention: new arrivals surface right at the
    // anchor point; older ones get pushed away as the window grows). An
    // EXISTING card's screen position is NOT static, though: every arrival
    // pushes it away from the anchored corner as the Column (and the
    // window's free top edge) grows — that's correct, standard toast-stack
    // behavior, not a bug; only the anchored bottom-right corner itself stays
    // fixed.

    // ── Layer-shell surface (Overlay, bottom-right corner) ──────────────────
    // Sized to its OWN content (implicitWidth/Height) — never full-screen —
    // so the surface never blocks clicks to whatever sits underneath unused
    // space. Anchored bottom+right only, so the window's free top edge is
    // what moves as the stack grows/shrinks — it grows UPWARD from the fixed
    // bottom-right corner; the anchored corner itself never moves, but every
    // existing card DOES get pushed upward/away from it as new ones land
    // there (standard toast-stack behavior — see the stack-order comment
    // above).
    // exclusiveZone 0 is the default posture (unlike the wallpaper's -1): it
    // still respects any OTHER surface's exclusive zone, though nothing is
    // currently anchored to the bottom edge to avoid.
    anchors { bottom: true; right: true }
    margins.bottom: 8
    margins.right: 12
    exclusiveZone: 0
    color: "transparent"
    visible: root.count > 0
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-notifications"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

    implicitWidth: 340
    implicitHeight: Math.max(1, stack.implicitHeight)

    Column {
        id: stack
        width: 340
        spacing: 8

        Repeater {
            model: notifServer.trackedNotifications
            delegate: WidgetSlot {
                required property var modelData
                width: stack.width
                notes: root.notes
                bridge: root.bridge
                stagingEngine: root.stagingEngine
                slot: "notifications"
                extraProps: ({ "notification": modelData })
                fallback: notifCardComp
            }
        }
    }
}
