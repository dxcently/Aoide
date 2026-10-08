# cadenza — coverage checklist

Every surface a song can dress today, every bridged capability sonata
answers on it, and cadenza's answer. Built from `pkgs/lyra-shell/qml/slots.md` (the wired anchors), `shell.qml`, and a read of sonata's
widget bodies for what each one reaches. Status: `design` → `preview` →
`live` → `committed`.

## Anchored slots

| slot | anchor · host | bridged capabilities to cover | cadenza answer | status |
|---|---|---|---|---|
| `bar` | WidgetSlot · shell.qml PanelWindow (`aoide-bar`), extras `shared` `powermenu` `dock` `stagingEngine` | Hyprland workspaces + active window; `shared` session counts; `powermenu.toggle()`, `dock.toggle()`; Pipewire sinks/sources + mute + default switch; Bluetooth adapter/devices; UPower battery; Networking; SystemTray (SNI); clock; `stage/projects.json` + graph.json `workspaces[].project` + the `workspaceaction` line (bind/unbind a workspace) | the switchboard line (intent §3.1, §3.2), embedding the `ricemode` slot between TRAY and the clock; the binding is the jack insight pane's PROJECT row — chips, `[clear]`, an inline `[+ new]` name, each click one `workspaceaction` pinned to that jack. **Honest partial:** the shell's ShellBridge answers no `workspaceaction`, so the line goes fire-and-forget through `sendCommand` (success shows when graph.json rewrites; a refusal is silent, the amber pending mark clears after 5s); a bridge with `workspaceAction(fields, cb)` gets its `ok:false` shown as one dim line | design |
| `calendar` | WidgetSlot · inside the bar | month grid | `cal` pane from the clock cell | design |
| `ricemode` | WidgetSlot · inside the bar (`checks.bar-ricemode`) | `bridge.riceMenu`, `bridge.riceMode`, `bridge.riceDraft` + `stage/mode.json` (`livery.riceMode`, `modeRaw.draft`) | the RICE cell (intent §3.1) — cadenza's own body, the kit icon and word on the character cell, a chevron marking the menu; any click opens it, and the cell never switches. The kit Pane lists the runtime songs, the declarative row, and the staged song's drafts; a row sends one `riceMode`/`riceDraft` (dim `…`, clicks swallowed until `livery.riceMode`, the draft or the song changes, or 10s), and the Pane has no inner glow | design |
| `dock` | SurfaceSlot · shell.qml (`aoide-dock`), extras `shared` `stagingEngine`; `toggle()` from the bar and `SUPER+G` | toggle contract; `openTab(tab)` from the bar cells | the tabbed board: OVERVIEW · project tabs · SYS · NOTIF (intent §3.3); closes by `[x]`, Escape, `SUPER+G`, the showing tab's bar cell, or a click off it (the `aoide-dock-scrim` catcher) | design |
| `conductor` | WidgetSlot · sonata's dock | `stage/sessions.json` + `hooks.json` + `herald.json` summonses + `graph.json` `spawned` edges; `bridge.focusSession`, `sessionAction`, `traceSession`, `recheckSessions`, `heraldverdict` | not authored — the board's agent cards (`BoardCards`, intent §3.3 "The cards") on OVERVIEW (grouped by project) and each project tab: every published field sonata's card shows, subagents on tree limbs, the summons lane with `heraldverdict`, click → `focusSession`, the action line → `sessionAction`. Not drawn: the `traceSession` poll (the card reads the roster's `say`/`tool`) and the `recheckSessions` reap button | preview |
| `terminals` | WidgetSlot · sonata's dock | sessions.json (one card per window, the agent in it winning), `focusSession` | not authored — the board's terminal cards (`BoardCards`) on OVERVIEW and each project tab: agent · state · jack · up · cwd, click → `focusSession` | preview |
| `usage` | WidgetSlot · sonata's dock | `state/usage.json` (`livery.usagePath`) incl. its opt-in `ollama` block, `bridge.refreshUsage` | not authored — the SYS tab ACCOUNT pane (Claude gauges + the `ollama` month gauge when present) | design |
| `meters` | WidgetSlot · sonata's dock | CPU/mem (the paths sonata's meters reads) | not authored — the SYS tab MACHINE pane (`/proc/stat`, `/proc/meminfo`) | design |
| `power` | WidgetSlot · sonata's dock | power vitals (sonata's `routePath` FileView), UPower | not authored — the bar `BAT` cell + its pane | design |
| `herald-center` | WidgetSlot · sonata's dock | `stage/herald.json`, `heralddismiss`, `heraldverdict` | not authored — the NOTIF tab | design |
| `herald` | SurfaceSlot · shell.qml | `stage/herald.json`, `heralddismiss`, `heraldverdict` | borderless toast (intent §3.7) | design |
| `powermenu` | SurfaceSlot · shell.qml (`aoide-powermenu`) | `{cmd:"power", action}` | prompt pane (intent §3.6) | design |
| `launcher` | SurfaceSlot · shell.qml (`aoide-launcher`), extras `clipboard` `ledger` | apps, `AoideClipboard`, `GrimoireLedger` (`song/stage/grimoire.json`) | command pane (intent §3.5): fuzzy search over names + desktop ids, and a digits-only query that finds nothing names row n (Enter fires it) | design |

## Carried but not anchored (the shell still owns the surface)

| slot | why no cadenza body now |
|---|---|
| `wallpaper` | **answered** — cadenza's `widgets/wallpaper.qml` is the board drawn live (the lane anchors the slot inside the per-screen Background surface, `slots.md`): copper from `CoverPcb`, the light over it (intent §3.9). The still PNG (`cover/pcb-<w>x<h>.png`, staged with `lyra cover set`) stays as what draws when no widget is there. |
| `wallpaper-picker` | no `SurfaceSlot` wired; the shell's `AoideWallpaperPicker` (SUPER+W) draws. Revisit when anchored. |

## Shell-owned, not dressable by a song today

Lockscreen, OSD, greeter, the `lyra secrets ask` / `lyra pair ask|show`
dialogs, the `*Preview` rigs. They read `livery.*`, so cadenza's key
recolours them; their shapes are out of the song's reach.

## New surfaces waiting on core seams (core-seams proposal, not built)

| surface | reads (proposal's names) | slice | today |
|---|---|---|---|
| jack project labels | `graph.json` `workspaces[].project` | S1 (+S3 for the block) — **built** (core main 2b21702) | read from core when graph.json carries the block; derived (the project anchoring the jack's sessions) only when it does not |
| tie lines | `graph.json` `ties[]` (`kind` `project` / `spawned`) | S3 — **built** | core's ties (a negative end dropped); else derived: `spawned`/`anchors` edges + session window → Hyprland workspace |
| lamps | `activeAt` on `workspaces[]` / `ties[]` | S3 — **built** | core's `activeAt`; else derived: `hooks.json` `updatedAt` advancing |
| working pulse | `hooks.json` phase `working` + session → jack (`workspaces[].sessions`) | S3 for the join | built on `hooks.json`; the jack join is derived (window → toplevel) until S3 |
| send lamps | `graph.json` `sends: [{from, to, at}]` (the conductor's recent sends, newest last) | a core ask; no slice named in core-seams yet | nothing: absent `sends` runs no lamp (the switchboard fixture carries a ring) |
| live cover (agents lit on the board) | a song-owned `wallpaper` slot anchor in the shell | **built** — `widgets/wallpaper.qml` + `widgets/Trace.js` | the board's light: one ring per live session in its state colour, pulses scaled by working/awaiting, a burst on a `hooks.json` advance |
| jack insight + SYS per-jack numbers | `state/usage/now.json` (`by: "workspace"`) | S5 tokens/cost, S6 CPU/mem, S7 history | jack pane: honest empty (§3.4); SYS per-jack: hidden (`hasJackUsage`) |
| board feed + OVERVIEW mail | `aoide project board` via a shellbridge read op | S8–S10 | hidden (`hasBoardFeed`, `hasMailRead`); a project tab is its agents + terminals |
| composer → agent | `{cmd:"boardpost", to:{session}}` | S11 | hidden (`hasBoardPost`) |
| composer → project | `{cmd:"boardpost", to:{project}}` | S12 (after §15 ML1) | hidden (`hasBoardPost`); agent targets only once shown |

**The derivation bend.** Until S3 publishes `ties`/`activeAt`, the bar
joins three published facts itself: `graph.json` edges, `sessions.json`
`windowAddress`, and Hyprland's toplevel workspaces (intent §3.2). It is
paint over published data, never a new fact, and it yields to the core's
fields the moment they exist. Delete the join when S3 lands.
