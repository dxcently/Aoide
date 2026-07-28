---
type: entity
created: 2026-07-26
updated: 2026-07-28
aliases: [Melete daemon, melete.service]
tags: [aoide, melete, agent, coding-agent, harness, integration]
---

# Melete

The coding harness AoideOS **integrates and launches** — the **doer**. Melete is
an independently-owned, long-running daemon (its own self-updating runtime; the
repo ships only the launcher `modules/dendrites/melete.nix` + the
`melete-adapter`, not Melete's source) that dispatches autonomous coding runs,
executes shell commands, drives a headless `claude` CLI, and reaches out to
GitHub and a rented fleet. It is the engine that makes AoideOS a declarative
[[Widget-Maker|widget maker]]: because Melete writes code, new integrations are
*generated*, not selected from a plugin menu. The relationship runs both ways —
`melete aoide …` routes into Aoide's CLI trunk, so Melete can also drive Aoide.

**The muse.** Melete, Mneme, and **Aoide** are the three Boeotian (pre-Olympian)
Muses: **Melete** = practice/rehearsal, **Mneme** = memory, **Aoide** = song.
The division of labor is literal — Melete *does* (dispatches, executes, builds),
[[Mneme]] *remembers* (the vault and its history), and Aoide is the *song*: the
performed desktop the other two act upon. Aoide is the third muse the pair were
missing.

## What it is

- **A harness, not a shell.** There is no REPL to drop into. The command surface
  is a fixed set of verbs (`code`, `maintain`, `store`, `gh`, `runs`, `recur`,
  `self-update`, `skill`, …), each a one-shot call over a long-lived `serve`
  daemon (`melete.service`) that holds the scheduler, the run registry, and the
  MCP surface. A shell only ever appears *inside* a job — policy-gated, and the
  job's own, never Melete's prompt.
- **Three concentric tiers** ([[Governance|opinionated by design]]): a **Rust
  core** (every primitive — audited, trusted), **Rune** scripts in the middle
  (compile-checked, can only call core primitives — small enough that reading one
  *is* auditing it), and **Python** as an opt-in, gated outer ring. Power at the
  center, instability pushed to the edge.
- **Reached over MCP** as a connector; ~54 tools on the surface. In the Magi
  fleet it runs as twin instances (`Melete:Sakaki`, `Melete:Osaka`). The Melete
  connector is the doer harness — distinct from the [[Mneme]] connector (the
  vault door) and the Aoide connector (the desktop/system frame's own management
  surface). See [[Agent-Interface]].
- **Declarative surface:** one `config.toml`; self-update swaps which Nix store
  path the service points at (a `canary` channel — a flake-generation rollback in
  spirit).

## Feature set

| Chain | Capabilities |
|---|---|
| **Coding dispatch** | `run_code_task` (one objective/repo → checkout, autonomous session, commit + PR); `run_code_batch` (related objectives, one warm checkout); `schedule_code_task`/`schedule_code_batch` (delayed); `schedule_recurring` (fixed cadence); `schedule_rune_script` (ephemeral inline) |
| **Run control** | `steer_run` (redirect live), `stop_run`, `recycle_run` (resume a terminal run), `job_status`, `list_runs`, `get_run_audit` |
| **Retrospection** | `scorecard` (did PRs land clean / rework / revert), `policy_analytics` (what the classifier fired on and how it resolved) |
| **Shell & fleet** | `run_shell` (bash on host), `ssh_exec` (into a declared `fleet.projects` box over SSH), `edit_local_script`; GitHub ops (`gh_read_file`, `gh_propose_change`, `gh_merge_pr`, `gh_list_issues`, …); Vultr fleet (`vultr_list_instances`, `fleet_inventory` drift detection) |
| **Skills** | Rune skills — `list_skills`, `run_skill_task`, `run_library_call` |
| **State & data** | restic backups (`backup_now`, `list_backups`, `restore`); attachments (`store_url`, `read_attachment`, `search_attachments`); connector management; box auth; daemon self-update |
| **Vault maintenance** | `run_maintenance` — harvest / cross-link / weed / reconcile passes over [[Mneme]] |

**Two guarantees worth naming** (they align with Aoide's own [[Governance]]):

- **No lockout / no single point of failure** — every dependency has a second
  way in: Telegram down → fall back to MCP; Claude quota exhausted → fall back to
  OpenRouter; bad config → a sentinel rolls it back.
- **Gated where autonomous, attended where live** — a **policy classifier** gates
  `run_code_task`/`run_code_batch`; `run_shell`/`ssh_exec` are *not* classified
  (the assumption is a human is watching turn by turn). Coding runs get
  suspend/resume + log-replay memoization for free at the core, so a resumed run
  never double-fires a completed side effect.

## Its part in the Aoide system

- **The extension engine.** Melete is why Aoide is a [[Widget-Maker]]: ask for an
  integration and Melete writes the dendrite (nix) + widget (QML) + adapter, then
  previews and — on your approval — adopts it. Same gated loop as [[Self-Ricing]].
- **On the event spine.** A `melete-adapter` consumes [[aoided]]'s neutral event
  stream and translates events into job dispatch ([[Desktop-Architecture]]). It
  runs as the `aoide-melete-adapter` systemd user unit via `aoide adapter melete
  --run`; its allow-list arrives through the `AOIDE_ADAPTER_SUBSCRIBE` env var —
  default-deny, comma-separated event *classes* (`audit`/`gate`/`rice`/
  `content`/`notification`; unrecognized names are silently dropped, allowing
  nothing) — and forwarded notifications reach it as metadata only
  (`{ actionId, appName }`), never the raw body — the security boundary as a
  real code path ([[Codebase]]).
- **Behind the exemplar features** ([[Feature-Set]]): its Telegram streaming backs
  the notification → messaging bridge; `schedule_*` + `recur` back the
  scheduled-jobs/timers widget; `ssh_exec` + `fleet_inventory` back fleet
  management; `run_code_task` powers autonomous coding.
- **Governance fit.** Melete proposes; the user admits; git records. Its
  attended-vs-gated split and no-background-self-updater stance are the same house
  policy Aoide enforces at the rebuild gate.

*Source: the Magi GNOSIS `Melete-Mneme` sub-wiki (Overview, Philosophy,
Melete-Mneme-Interaction, Vault-Access, Coding-Dispatch), read via the
`Mneme:Sakaki` connector.*

## Related

- [[Mneme]]
- [[Widget-Maker]]
- [[Feature-Set]]
- [[aoided]]
- [[Desktop-Architecture]]
- [[Governance]]
- [[Wiki-Protocol]]
- [[Codebase]]
