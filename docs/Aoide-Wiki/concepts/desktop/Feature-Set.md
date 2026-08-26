---
type: concept
created: 2026-07-26
updated: 2026-08-25
tags: [aoide, features, integration, melete, mneme]
---

# Feature Set — Batteries Included

What a fresh **AoideOS** clone comes with. AoideOS wires the whole
personal-computing stack together — the desktop
([[Desktop-Architecture]]) and orchestrator ([[aoided]]) it **owns**, plus the
independently-owned coding harness **[[Melete]]** and knowledge server
**[[Mneme]]** it **integrates and launches** (adapters + launchers, not vendored
code) — and it exposes every one of their capabilities through the same system
hooks (event stream → adapters → widgets; [[Agent-Interface|CLI trunk + MCP
façade]]). Many capabilities below are Melete's, *surfaced* through AoideOS —
see the "Provided by" column.

Every integration is a dendrite ([[Snowflake-Anatomy]]): opt-in, flag-toggled,
themed by [[livery]], and — where it touches the outside world —
`enable = false` by default and behind the [[Governance|gate]].

This list is what ships. Because its agent ([[Melete]]) is a coding agent,
new integrations are generated declaratively, on demand — see
[[Widget-Maker]]; everything below is an exemplar of that capability.

The desktop, agent, pipeline, and governance items are grounded in
[[references/AOIDE-HANDOFF]]. The messaging bridge, Cloudflare/Tailscale fleet
exposure, and the scheduled-jobs widget are planned — specified here as
intended features, not present in `aoide schema --json`'s command surface
today; they extend the handoff rather than describe shipped commands.

## The bundle

| Ships | What it is | Page |
|---|---|---|
| Aoide framework | compositor · shell · aoided · pipeline · rice engine | [[Overview]] |
| Melete | agent + coding harness: autonomous code tasks, maintenance, scheduling, fleet SSH | [[Melete]] |
| Mneme | knowledge server: vault notes, conventions, skills, search, versions | [[Mneme]] |
| claude CLI | the first-class agent path | [[Agent-Interface]] |

## Feature groups

### 1. Notifications → messaging bridge

**Status:** planned; no `notify-bridge` dendrite or `aoide notify` command in
`aoide schema --json` yet.

The [[Quickshell]] notification daemon (`org.freedesktop.Notifications`) is the
single source of desktop events. A **notification-bridge dendrite** mirrors
chosen event classes to a messaging app — **Telegram first**, pluggable (Signal /
Matrix / Discord / …).

- Outbound: Melete already streams job + commit updates to Telegram; the
  bridge generalizes this to any desktop notification, filtered by [[aoided]]'s
  default-deny-per-class policy so an OSD flood can't spam your phone.
- Inbound: replies from the messaging app become agent actions, as untrusted
  data, never executed as commands (the hard trust boundary from
  [[Desktop-Architecture]]).
- Surfaced as a bar connection/notification widget; a planned `aoide notify …` command (not yet in the command schema).
- Default `enable = false` (an external surface); target app + token are
  user-provided.

### 2. Fleet & networking — Tailscale + Cloudflare

**Status:** Tailscale's network MCP and Melete's fleet tools are shipped; the
`aoide fleet` command, the fleet widget, and Cloudflare public exposure are
planned, not present in `aoide schema --json`.

Remote access and controlled exposure are first-class, user-gated features.

- Tailscale: tailnet membership; network MCP served over the tailnet
  (user-only, never agent-enabled — [[Agent-Interface]]); `ssh_exec` across fleet
  hosts (Melete).
- Cloudflare: tunnels / funnel to expose chosen services publicly; funnel as
  the alternative network-MCP door.
- Fleet management: inventory, drift check, backups, per-host command
  dispatch (Melete `fleet_inventory` · `ssh_exec` · `check_drift` · backups).
- Surfaced as a planned `aoide fleet …` command (not yet in the command
  schema); a panel fleet widget (host list + reachability).
- Default: exposure toggles are `enable = false`; every enable is a
  [[Governance|gated]] action.

### 3. Scheduled jobs & timers (with a widget)

**Status:** Melete's schedule tools (`schedule_code_task`,
`schedule_code_batch`, `schedule_recurring`, `schedule_rune_script`) are
shipped; the `aoide sched` command and the agenda/timers widget are planned, not
present in `aoide schema --json`.

The design surfaces **both** agent schedules and system timers in one place.

- Agent schedules (Melete): `schedule_code_task` / `schedule_code_batch`
  (delayed), `schedule_recurring` (fixed cadence), job chaining (`after` + `on`),
  one-shot `schedule_rune_script`.
- System timers: systemd timers surfaced read-only alongside.
- Widget: a Quickshell agenda/timers widget reads a `song/stage/*.json`
  state file and shows pending one-shots, recurring cadences, chained jobs
  ("waiting on `<id>`"), and system timers — live, themed by livery.
- Governance: scheduled *coding* runs still route their result through the
  rebuild gate; no background self-updaters ([[Governance]]).
- Surfaced as a planned `aoide sched …` command (list / create / cancel, not
  yet in the command schema) + the widget.

### 4. Knowledge & content (Mneme)

- The vault is served through Mneme's API and ingested through the
  [[Content-Pipeline]] **approve gate** — raw filesystem access to a vault is
  never permitted.
- Ships: conventions, agent skills (`skill_body`), full-text search, per-note
  version history, canvases.
- Project wikis are minted and kept by the [[Wiki-Protocol]] (Mneme/Melete-owned);
  Aoide's own wiki is the self-managed exception.

### 5. Coding agent (Melete)

- Autonomous **code tasks / batches** → verified against the toolchain → PR,
  streamed to messaging.
- Vault **maintenance** passes (harvest / cross-link / weed / reconcile).
- Audit, recycle, scorecard, policy analytics on every run.
- Exposed through the same [[Agent-Interface|CLI trunk]] — the `melete aoide …`
  passthrough routes through the one trunk, so there is no second capability
  surface to keep in sync.

### 6. Agent-session terminal commander (with a widget)

A live roster of the terminals running agents — spawned by [[Melete]] or any other
agent — with **jump by click or keybind**. Full page: [[Terminal-Commander]].

- Watches: new agent terminals via a watcher dendrite on [[aoided]]'s event
  stream; sessions register their window address with [[shellbridge]] at spawn.
- Shows: one row per session (agent · repo/cwd · state · elapsed), live from
  `song/stage/*.json`; an `awaiting-input` row can chime or hit the messaging
  bridge.
- Jump: click → `hyprctl dispatch focuswindow address:…` (one hop); or a
  [[Hyprland]] keybind to cycle agent terminals / pop the roster.
- Surfaced as the terminal-commander widget + the real `aoide graph
  session …` command group.

## How every feature hooks into the system

One spine, so a new integration is always the same shape:

```
 external surface           dendrite / adapter       system spine             user-facing
 (Telegram, tailnet,   ──►  melete-adapter,     ──►  aoided event stream  ──► Quickshell widget
  Cloudflare, vault,        notify-bridge,           (default-deny)           (reads song/stage/*.json)
  systemd timers)           fleet-adapter, …         policy · gate · audit    CLI: aoide <cmd>
```

- In: external events enter as *data* through an adapter; [[aoided]] applies
  default-deny-per-class before anything reaches an agent.
- Out: state is written atomically to `song/stage/` by [[shellbridge]];
  widgets read it; nothing in QML speaks an agent protocol.
- Control: every capability is one `aoide <cmd>` (+ generated MCP façade);
  every mutation and exposure passes the [[Governance|gate]] and lands in the
  single audit log.
- Toggle: each is a dendrite flag; external-facing ones default off.

## Feature matrix

Shipped exemplars plus planned extensions — the [[Widget-Maker|agent generates
more]] on demand. Rows marked *(planned)* have no command yet in `aoide schema
--json`'s command surface.

| Capability | Provided by | Surfaced as | Default | Gate |
|---|---|---|---|---|
| Notification → messaging *(planned)* | notify-bridge + Melete | widget · `aoide notify` | off | policy + user token |
| Network MCP (tailnet) | Tailscale + Melete | — | off | user-only |
| Public exposure (funnel/tunnel) *(planned)* | Cloudflare | `aoide fleet` | off | gated |
| Fleet SSH / inventory / drift *(planned)* | Melete | fleet widget · `aoide fleet` | off | gated |
| Scheduled coding jobs *(planned)* | Melete | agenda widget · `aoide sched` | — | rebuild gate on result |
| Recurring jobs / chains *(planned)* | Melete | agenda widget | — | gated |
| System timers view *(planned)* | systemd | agenda widget | on (read-only) | — |
| Vault knowledge | Mneme | pipeline · `aoide …` | via approve gate | approve gate |
| Autonomous code tasks | Melete | messaging stream · PR | — | rebuild gate |
| Self-ricing | livery + facets | `lyra rice` (`lint`/`stage`/`compose`/`draft`/`mode`/`take`/`back` real; `declare`/`transpose` exit-64 stubs) | on | rebuild gate |
| Agent-session terminal commander | shellbridge + aoided watcher | widget · `aoide graph session` · click/keybind jump | on | — |

## Related

- [[Widget-Maker]]
- [[Overview]]
- [[Full-Architecture]]
- [[Desktop-Architecture]]
- [[Agent-Interface]]
- [[Content-Pipeline]]
- [[Governance]]
- [[Self-Ricing]]
- [[Melete]]
- [[Mneme]]
