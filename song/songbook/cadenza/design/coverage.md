# cadenza — coverage checklist

Every surface a song can dress today, every bridged capability sonata
answers on it, and cadenza's answer. Built from `modules/facets/quickshell/
qml/slots.md` (the wired anchors), `shell.qml`, and a read of sonata's
widget bodies for what each one reaches. Status: `design` → `preview` →
`live` → `committed`.

## Anchored slots

| slot | anchor · host | bridged capabilities to cover | cadenza answer | status |
|---|---|---|---|---|
| `bar` | WidgetSlot · shell.qml PanelWindow (`aoide-bar`), extras `shared` `powermenu` `dock` `stagingEngine` | Hyprland workspaces + active window; `shared` session counts; `powermenu.toggle()`, `dock.toggle()`; Pipewire sinks/sources + mute + default switch; Bluetooth adapter/devices; UPower battery; Networking; SystemTray (SNI); `bridge.toggleRiceMode` + `stage/mode.json`; clock | the switchboard line (intent §3.1, §3.2) | design |
| `calendar` | WidgetSlot · inside the bar | month grid | `cal` pane from the clock cell | design |
| `dock` | SurfaceSlot · shell.qml (`aoide-dock`), extras `shared` `stagingEngine`; `toggle()` from the bar and `SUPER+G` | toggle contract | the tabbed board: OVERVIEW · project tabs · SYS · NOTIF (intent §3.3) | design |
| `conductor` | WidgetSlot · sonata's dock | `stage/sessions.json` + `hooks.json`; `bridge.focusSession`, `sessionAction`, `traceSession`, `recheckSessions`, `heraldverdict` | not authored — the board OVERVIEW AGENTS pane + project rails draw agents | design |
| `terminals` | WidgetSlot · sonata's dock | sessions.json (terminal sessions), `focusSession`, `focusWindow` | not authored — OVERVIEW TERMINALS pane + project rails | design |
| `usage` | WidgetSlot · sonata's dock | `state/usage.json` (`livery.usagePath`), `bridge.refreshUsage` | not authored — the SYS tab | design |
| `meters` | WidgetSlot · sonata's dock | CPU/mem (the paths sonata's meters reads) | not authored — the SYS tab | design |
| `power` | WidgetSlot · sonata's dock | power vitals (sonata's `routePath` FileView), UPower | not authored — the bar `BAT` cell + its pane | design |
| `herald-center` | WidgetSlot · sonata's dock | `stage/herald.json`, `heralddismiss`, `heraldverdict` | not authored — the NOTIF tab | design |
| `herald` | SurfaceSlot · shell.qml | `stage/herald.json`, `heralddismiss`, `heraldverdict` | borderless toast (intent §3.7) | design |
| `powermenu` | SurfaceSlot · shell.qml (`aoide-powermenu`) | `{cmd:"power", action}` | prompt pane (intent §3.6) | design |
| `launcher` | SurfaceSlot · shell.qml (`aoide-launcher`), extras `clipboard` `ledger` | apps, `AoideClipboard`, `GrimoireLedger` (`song/stage/grimoire.json`) | command pane (intent §3.5) | design |

## Carried but not anchored (facet still owns the surface)

| slot | why no cadenza body now |
|---|---|
| `wallpaper` | shell.qml still hosts the facet's own wallpaper; the slot anchor lands with a later phase. Cadenza's cover is `null` (solid `palette.bg`), so the facet's solid fallback is already the right picture. |
| `wallpaper-picker` | no `SurfaceSlot` wired; the facet's `AoideWallpaperPicker` (SUPER+W) draws. Revisit when anchored. |

## Facet-owned, not dressable by a song today

Lockscreen, OSD, greeter, the `lyra secrets ask` / `lyra pair ask|show`
dialogs, the `*Preview` rigs. They read `livery.*`, so cadenza's key
recolours them; their shapes are out of the song's reach.

## New surfaces waiting on core seams (core-seams proposal, not built)

| surface | reads (proposal's names) | slice | today |
|---|---|---|---|
| jack project labels | `graph.json` `workspaces[].project` | S1 (+S3 for the block) | not drawn |
| tie lines | `graph.json` `ties[]` (`kind` `project` / `spawned`) | S3 | not drawn |
| lamps | `activeAt` on `workspaces[]` / `ties[]` | S3 | not drawn |
| jack insight + SYS per-jack numbers | `state/usage/now.json` (`by: "workspace"`) | S5 tokens/cost, S6 CPU/mem, S7 history | honest empty |
| board feed + OVERVIEW mail | `aoide project board` via a shellbridge read op | S8–S10 | honest empty |
| composer → agent | `{cmd:"boardpost", to:{session}}` | S11 | disabled |
| composer → project | `{cmd:"boardpost", to:{project}}` | S12 (after §15 ML1) | agent targets only |
