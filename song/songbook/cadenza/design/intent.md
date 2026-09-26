# cadenza — Design Intent

**Song:** cadenza (composed from `sonata`, then drawn from nothing — no
sonata widget body or gadget slot is reused; cadenza draws its own)
**Register:** a hacker console on a green-phosphor CRT, wired like a
telephone switchboard. Box-drawn termui panes with the title cut into the
top rule, braille line charts, block sparklines, bar gauges, a monospace
grid everywhere; workspaces are jacks on a switchboard, joined by tie lines,
lit by lamps.
**References:** `refs/ref-termui.png` (the pane grammar: box rules, green
titles, braille/dot charts, sparklines, a gauge) and `refs/ref-crt.jpg` (the
light: green phosphor on near-black, a faint bloom, nothing else).
**Polarity:** dark. Needs a livery field the facet does not read yet (§5).
**Cover:** none. The wallpaper note is `null`, so the stylix facet bakes a
solid field from `palette.bg` — CRT black. The screen is the tube.
**Glass:** none. No compositor blur, no hyprglass (§5).

Companion files: `coverage.md` (every bridged surface and cadenza's answer),
`refs/` (the two references). Data names follow the core-seams proposal
(workspace binding, `ties`, `state/usage/now.json`, the board, `boardpost`).

---

## 1. The aesthetic in one paragraph

Everything is a terminal. Every surface is a termui **pane**: a single-line
rule (`┌─ TITLE ───┐ │ │ └───┘`) with the title cut into the top edge in
phosphor green, content on a monospace character grid, no radius, no glass,
no gradient, no drop shadow. Colour is spent the way a 16-colour terminal
spends it: green is the default voice, amber is the one live thing, and a
small set of terminal contrast colours each carry exactly one kind of
highlight. The bar is a switchboard: workspaces are **jacks**, the real
edges between them are **tie lines**, the bar's bottom rule is the **trunk**
every tie hangs off, and activity lights a **lamp** that runs down a line.
At rest nothing moves.

### Vocabulary (one name per thing)
| word | what it is | layer |
|---|---|---|
| **jack** | a workspace, drawn `[3]` on the switchboard | paint only; the data noun is core's *workspace* (an integer) |
| **tie** | a real edge between two workspaces, kind `project` or `spawned` | core's noun (`graph.json` `ties`); the paint draws it as a tie line |
| **trunk** | the bar's bottom rule; every tie line drops from it | paint only |
| **lamp** | the lit dash that runs a line when its `activeAt` advances | paint only; core publishes `activeAt` |
| **board** | the dock: the per-project message board | core's noun (`aoide project board`) |
| **pane** | one termui box | paint only |

"node" is never used for a workspace (it means a mesh machine); "trace" and
"channel" are never used for paint (they mean the session trace and the
Conductor Channel).

## 2. Grammar — the drawing vocabulary

### Faces and the grid
- One face: **JetBrainsMono Nerd Font Mono** (installed; box drawing and
  block elements and braille U+2800–28FF all native, no fallback).
  No proportional text anywhere.
- Every pane lays out on a character cell measured once from `FontMetrics`
  (never hard-coded); pane sizes are whole cells, so a rule always closes.
- Three type tiers: **title** (bold, accent, UPPERCASE in the rule),
  **body** (regular, fg), **dim** (regular, base03 — labels, axes, times).

### Pane chrome
```
┌─ AGENTS ─────────────── 3/5 ┐     title cut into the top rule, left
│ ● rook      working  12s    │     optional right-hand stat in the rule
│ ◐ minerva   awaiting  2m    │
└─────────────────────────────┘
```
- Rules are 1px `Rectangle` hairlines (`base03` at rest, `accent` when the
  pane has focus), NOT box-drawing text — text rules cannot close at
  fractional scale. Box-drawing glyphs are used INSIDE panes (tree limbs
  `├─ └─`, separators) where the cell grid is guaranteed.
- A **borderless block** (termui's magenta one) holds a single message that
  is not a pane — a herald toast, one board item.
- Panes are opaque `base00` at 0.94. Black glass, not frost.

### Instruments (termui's widgets, re-drawn)
| instrument | glyphs | used by |
|---|---|---|
| gauge | `█▉▊▋▌▍▎▏` eighth-blocks on a `░` track, % right-aligned | CPU, memory, battery, context window, volume |
| sparkline | `▁▂▃▄▅▆▇█` one column per sample | per-jack CPU, tokens/min |
| braille chart | U+2800 block, 2×4 dots per cell | cost over a range |
| bar chart | full-block columns, value on the base | token share per workspace/project |
| list | `[n]` index in dim, text in fg; the SELECTED row's index turns amber (the one live thing) | launcher, tray, sinks |
| state lamps | `●` working · `◐` awaiting · `○` idle · `■` stopped · `✕` failed | sessions everywhere |

### Colour — roles and highlights (hexes in `rice.nix`)
The ground and the ramp are phosphor; each highlight colour has ONE job, so
a colour on screen always means the same thing:

| colour | slot | job |
|---|---|---|
| phosphor green | fg / base05 | body text, idle rules, chart ink |
| bright phosphor | accent / base0B | titles, the focused pane's rule, the active jack |
| dim phosphor | base03 | axes, labels, rules at rest, timestamps |
| **amber** | hot / base0A | the ONE live element: the lamp, the traced session, a text cursor. Never two amber things at rest |
| **red** | urgent / base08 | blocked, failed, low battery, a summons waiting on you |
| **orange** | base09 | warnings short of trouble: costs past a threshold, a `costPartial` figure |
| **cyan** | base0C | numbers worth reading: counts, token totals, durations |
| **blue** | base0D | paths, project names, workspace numbers in running text |
| **magenta** | base0E | a person's words: your posts, mail from a human |
| **white-hot** | base07 | a search match / the selected row's text |

Every text colour clears 4.5:1 against both `base00` and `base01` (WCAG
relative luminance):

| colour | hex | vs base00 | vs base01 | vs base02 |
|---|---|---|---|---|
| fg / base05 | `#86f2a0` | 14.46 | 13.61 | 11.55 |
| accent / base0B | `#39ff6a` | 14.90 | 14.03 | 11.90 |
| dim / base03 | `#4a905d` | 5.17 | 4.86 | 4.13 |
| mid / base04 | `#62b377` | 7.82 | 7.36 | 6.25 |
| amber / base0A | `#ffb000` | 10.89 | 10.25 | 8.70 |
| red / base08 | `#ff4d4d` | 6.10 | 5.74 | 4.87 |
| orange / base09 | `#ff7a2e` | 7.67 | 7.22 | 6.13 |
| cyan / base0C | `#3fe0d0` | 12.15 | 11.43 | 9.70 |
| blue / base0D | `#4aa8ff` | 7.92 | 7.45 | 6.33 |
| magenta / base0E | `#d65cff` | 6.49 | 6.11 | 5.19 |
| bright / base06 | `#b6ffc8` | 17.22 | 16.21 | 13.75 |
| white-hot / base07 | `#e4ffea` | 18.80 | 17.70 | 15.02 |

`dim` is the floor: it carries labels and timestamps, so it is held just
above 4.5 on the ground and the block fill. `mid` sits one clear step above
it. On the `base02` selection band, dim text falls to 4.13, so a selected
row draws its text in `match`, never `dim`. `borderInactive` (`#2e5a3a`) is
a window border, not text, and stays darker.

### Motion
- At rest, nothing animates. No blinking cursor unless a field has focus.
  No scanline roll, no flicker, no idle shimmer.
- **Lamp:** when a jack's or a tie's `activeAt` advances between two reads
  of `graph.json`, a 12px amber dash runs the tie line (or drops from the
  trunk into the jack) in 600ms linear, then the line goes dark. One
  `NumberAnimation` on one `Rectangle`; at most one lamp in flight per line,
  later advances coalesce into it.
- **Reveal:** a pane draws its rule clockwise in 160ms then fills (the CRT
  "paint"); closes in 120ms. Within sonata's documented bands.
- **Board lines** appear whole — no typewriter effect.

### Glow — the budget
Text should "glow a little, only if cheap". Three tiers, drawn by
`GlowText` (`design/kit.md` §3):

1. **Tier 0 (`outline`, the default):** the text's own `style: Text.Outline`,
   `styleColor` its colour at 0.18. There is no offscreen buffer and no
   shader.
2. **Tier 1 (`bloom`, titles only, baked):** a hidden copy of the title fed
   to one `MultiEffect { blur }`. It is rendered once and reused until the
   title text changes. Never on an animating item, never on a list delegate.
3. **Never:** an animated blur, a per-line blur on board rows, a
   full-surface bloom shader.

**Measured** on a board-shaped bench: a 200-row list of the board's row
anatomy (time, lamp, name, text with untrusted words) under 4 pane titles.
It ran in the preview canvas with its own root, 8s per window after a 6s
settle, sampling CPU from `/proc/<pid>/stat` and frame work
(sync+render) from `qt.scenegraph.time.renderloop`. Scroll is a continuous
contentY sweep of the whole list every 4s.

**These numbers are software-rendered and relative-only.** They come from
the offscreen canvas (`QT_QPA_PLATFORM=offscreen`, the Qt Quick *software*
renderer). They rank the tiers against each other; they are not the GPU cost
on the desktop.

| glow | idle CPU | idle frames | scroll CPU | scroll frame work p50 / p95 / max | RSS |
|---|---|---|---|---|---|
| off | 0.0% | 0 | 25.9% | 3 / 3 / 4 ms | 113–114 MB |
| tier 0 on rows and titles | 0.0% | 0 | 50.1% | 6 / 8 / 11 ms | 112–115 MB |
| tier 0 rows + tier 1 titles | 0.0% | 0 | 49.6% | 6 / 8 / 11 ms | 112–115 MB |
| tier 1 on every row (the "never") | — | — | 56.2% | 6 / 8 / 12 ms, uneven frame rate | 121 MB |

**Verdict.**

- At rest, every tier costs nothing: zero frames and 0.0% CPU. The static
  scene is not redrawn, blurred titles included.
- Tier 1 on titles is free relative to tier 0. The same scroll cost, the
  same memory, because the four blurred titles are baked once. It stays, on
  titles only.
- Tier 0 on scrolling rows roughly doubles the software renderer's per-frame
  work (2.6 → 6.3 ms mean). That is still well inside a 16.7 ms frame, with
  no dropped frames at 62 fps and p95 of 8 ms, so it does not trip the
  "visible hitch" line. Tier 0 therefore stays the default, board rows
  included, **provisionally**. The software renderer draws an outline as
  extra glyph passes, which is its worst case; the GPU number decides. Until
  a GPU run confirms it, a long scrolling list may pass `glow: "off"` on its
  rows at no loss to the look of its titles.
- Tier 1 on rows costs more CPU and ~6 MB more for 200 rows, and paces
  frames unevenly. It stays banned.

The GPU re-run is the same bench with the real canvas.

## 3. The surfaces

### 3.1 Bar — the switchboard (`bar`)
One 28px line in the tmux/termui idiom, left to right:
- `[⏻]` — the power key (opens `powermenu`).
- the **switchboard** (§3.2), the widest element.
- the active window title, dim, truncated.
- right cells, each `LABEL value` on the grid: `AGT 3/5`, `CPU 23%`,
  `$ 4.20`, `!2` — each opens the board on its tab (OVERVIEW, SYS, SYS,
  NOTIF); then `VOL 62%` · `BT` · `NET` (their own small panes), `BAT 88%`,
  the tray, `RICE stg` (the rice-mode toggle), and the clock `14:02:31`
  (the calendar pane).
- The bar's bottom rule is the **trunk**.

### 3.2 The switchboard (inside the bar)
- Workspaces keep their numbers: jacks `[1] [2] [3] …`. Occupied jacks are
  solid rule; empty jacks dim; the active jack accent; a jack with an
  awaiting/blocked session red.
- A bound jack carries its project after it, blue: `[2]aoide` (the core's
  `workspaces[].project`).
- **Tie lines** run in a 6px band under the jack row as orthogonal routes:
  drop from jack A, run along a lane, rise into jack B.
  - `project` tie: solid line.
  - `spawned` tie: dotted line.
  - Up to 3 lanes; a 4th tie collapses into a `+n` badge on the jack. The
    core publishes a clique for a project shared by 3+ jacks; the paint
    draws that as one bus lane touching all of them, not n² lines.
- Lamps run on `activeAt` (§2 Motion).
- Hovering a jack opens the **jack insight pane** (§3.4).
- **Until the seams land:** jack numbers, occupancy, active and urgent are
  real today (Hyprland + `sessions.json`). Tie lines, project labels and
  lamps are NOT drawn — no fake edges. The trunk is drawn, dark.

### 3.3 Dock → the board (`dock`, `aoide-dock`)
A right-edge pane, full height under the bar, tabbed. The dock is cadenza's
own board; it mounts none of sonata's gadget slots.

```
┌─ BOARD ─────────────────────────────────────────┐
│ OVERVIEW │ aoide │ melete │ mneme │ SYS │ NOTIF  │
├─────────────────────────────────────────────────┤
```

- **OVERVIEW** (first tab) — four panes on one screen: **AGENTS** (every
  live agent: lamp, name, project, state, age; focus/send actions),
  **PROJECTS** (registered projects, the jacks each is bound to, live
  counts), **TERMINALS** (conducted terminals: name, cwd, jack), **MAIL**
  (active mail threads: mailbox, subject, last sender, age).
- **one tab per project** — the project's board feed (chatter, mail,
  receipts, its summonses), with that project's own agents and terminals in
  a narrow rail on the right, and the composer at the bottom.
  ```
  │ 14:02 rook      ● turn settled               │ ● rook     │
  │ 14:02 minerva   ◐ Bash: cargo test           │ ◐ minerva  │
  │ 14:03 ↳ eidolon settled end_turn · 12 calls  │ ─ tty ──── │
  │ 14:05 khoa      re: phase 5 slice S8         │ ○ zsh [2]  │
  ├──────────────────────────────────────────────┴────────────┤
  │ to: aoide ▾ │ > _                                          │
  ```
- **SYS** — machine and spend on one tab: CPU/memory gauges and per-jack
  sparklines (`state/usage/now.json`), and token/cost usage — the account
  block (`state/usage.json`, real today) plus per-workspace/project totals
  from the new store.
- **NOTIF** — the herald: toasts and summonses as borderless blocks, quoted
  plain text, `[y] approve [n] deny` on a summons (the existing
  `heraldverdict` / `heralddismiss` commands, nothing else).
- Opened exactly as sonata's dock is (`toggle()` from the bar, `SUPER+G`);
  the bar's cells open it on a named tab.

**Untrusted text (house rule 4).** Every board `text`, `summary`, `body`,
`subject` is rendered `Text.PlainText`, never linkified, never actionable.
The composer is never pre-filled from an item.

**Until the seams land:**
| part | real today | honest empty until |
|---|---|---|
| OVERVIEW agents / terminals | `sessions.json` | — |
| OVERVIEW projects | `projects.json` (bindings column empty) | S1 for bindings |
| OVERVIEW mail | — | `no mail view — bridge not wired` until S9/S10 |
| project tabs (names) | `projects.json` | — |
| project feed | — | `no feed — bridge not wired` until S8/S10 |
| composer | drawn disabled, `post: bridge not wired` | S11 (agents), S12 (project) |
| SYS account usage | `state/usage.json` | — |
| SYS CPU/mem, per-jack tokens/cost | — | `no usage data — bridge not wired` until S5/S6 |
| NOTIF | `stage/herald.json` | — |

### 3.4 The jack insight pane
Dropped from a hovered jack: `JACK 2 · aoide`, then tokens and cost for the
sessions on that workspace (sparkline + total, `costPartial` in orange),
CPU and memory (gauges from `now.cpuPct` / `now.rssBytes`), and the session
list. Reads only `now.json`'s `by: "workspace"` row. Not a process viewer:
a `btop` row at the bottom launches the real one. Until S5/S6: the session
list is real (Hyprland + `sessions.json`), the numbers read `no usage data
— bridge not wired`.

### 3.5 Launcher (`launcher`, `aoide-launcher`)
A centred command pane: `> ` prompt, fuzzy list with `[n]` indices, modes as
tabs in the rule (`apps │ clip │ ledger`) — clipboard and the grimoire ledger
stay the facet data seams they are.

### 3.6 Power menu (`powermenu`, `aoide-powermenu`)
A centred pane like a shell prompt: `┌─ SHUTDOWN ─┐` with `[l] lock
[s] suspend [r] reboot [p] poweroff [o] logout`, keyboard-first (the letter
fires); the chosen row goes amber; a destructive row asks `y/N` inline.

### 3.7 Herald toast (`herald`)
A borderless block top-right under the bar: `herald ▸ <app>` dim, then the
quoted body; a summons carries `[y] approve [n] deny`.

### 3.8 Calendar (`calendar`)
A `cal`-style month grid in a pane from the clock cell, today in accent.

## 4. Preview fixtures

Unbuilt feeds are drawn with fixtures in the preview canvas ONLY
(`design/fixtures/<set>/`, passed as `lyra preview --fixture <dir>`), shaped
exactly after the core-seams proposal: `graph.json` with `workspaces` +
`ties` + `activeAt`, `state/usage/now.json`, and a board answer. The live
widget keeps its honest empty state until each slice lands; no adapter
reads a fixture path.

## 5. Hazards and open questions

- **Polarity (needs a core change).** The stylix module sets `polarity =
  lib.mkDefault "light"` and reads no polarity from the song; a song may
  only set `aoide.livery` (house rule 5). Proposed: an
  `aoide.livery.polarity` field (nucleus option, CONTRACTS §1, livery
  schema/lint), read by the stylix dendrite that phase 5 S5 creates in
  place of the facet. cadenza then declares `"dark"`.
- **Glass (needs a core change).** The compositor module loads hyprglass,
  its config block and the `blur on` layer rules unconditionally;
  `geometry.blurEnabled = false` only stops Hyprland's blur. Proposed, the
  dxflake pattern: when phase 5 S5 splits the compositor into dendrites,
  hyprglass becomes its own file gated on the song's livery (`blurEnabled`,
  or a `glass` field). dxflake's own
  `dendrites/compositor/hyprland/hyprglass.nix` also loads it whenever the
  quickshell module is on, so it needs the same gate.
- **Bar height** 28px vs sonata's 36; the facet reads it back, so it is the
  song's call.
- **Unbuilt seams** — every "bridge not wired" above names its slice.

## Iteration Log

- 2026-09-26 — composed from sonata; design drafted (this file,
  `coverage.md`, the livery key). Nothing staged.
- 2026-09-26 — khoa: no sonata gadgets (cadenza draws its own); the dock is
  a tabbed board — OVERVIEW first (agents, projects, terminals, mail), a
  tab per project, one SYS tab (CPU + usage), a NOTIF tab; terminal
  contrast colours for highlights; dark mode; no blur, no hyprglass.
  Nouns: switchboard (jack · tie · trunk · lamp), adopting core's `tie`.
