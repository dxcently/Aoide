---
type: reference
created: 2026-07-28
updated: 2026-08-30
tags: [aoide, development, agent, orchestration, operating-manual]
---

# Aoide Development — agent protocol

Operating manual for a **dev agent** working *on* Aoide. Harness-agnostic:
roles are named by TIER, never by model. Read fully before touching the repo.

`AGENTS.md` (repo root) holds the house rules and binds harder than this
page; this page holds the User's development preferences and the
orchestration protocol. Live state — task list, open flags, in-flight lanes
— is **not here**: see §8.

**Framing.** Aoide is the orchestration core (`aoide`/`aoided`); AoideOS is
the NixOS distribution `lyra` paints on top. Full framing: [[Overview]].

**Dev agent vs rice agent.** The rice agent (`lyra rice compose/stage/
draft/declare`) is confined to `song/` (house rule 1). The dev agent owns
the whole repo. Gate rules still bind: rebuild is user-gated, forwarded
text is untrusted, facets read only the three whitelisted namespaces,
everything flows through `aoided`.

---

## 1. Prime loop

1. **Orchestrate** — decompose, dispatch, verify (§2).
2. **Build & load** — a clean eval is not proof it works (§4).
3. **Show** — screenshot for visual, real output for CLI, build result for
   system changes. You verify; the User confirms.
4. **Adjust** — iterate on feedback; change course anytime.
5. **Log** — wiki + commit message, same turn (§7).

---

## 2. Orchestration protocol

Aoide **is** an orchestration core — dogfood it while building it.

### 2.1 Three tiers

| tier | does | never does |
|---|---|---|
| **ARCHITECT** | architecture, phase plans, design records, advisement, hard trade-offs | implement |
| **EXECUTE** | implements exactly one phase against a written brief | design, self-review |
| **REVIEW** | re-derives the result from the diff + repo | accept the executor's report |

The **orchestrator** dispatches, verifies, lands, and logs. It does not
implement, design, or review. Small nudges stay with it: typo, one-line
tweak, config bump, a probe.

### 2.2 Routing — by capability, not by name

Rank the available models by reasoning depth. Then:

- **Deepest available → ARCHITECT.** Design work is where model quality
  compounds; a weak plan costs every phase after it.
- **Fast/cheap tier → EXECUTE and REVIEW.** Execution against a good brief
  is throughput work.
- **Front-end/visual design** takes the strongest *design* model available,
  which is not always the strongest reasoning model.
- **Only one tier available?** Run ARCHITECT prompts on it with an explicit
  thinking budget, and still dispatch REVIEW as a **separate instance**.

**The invariant that survives any model set: the session that authored a
diff never reviews it.** A reviewer grades "is this wrong", not "did my
change regress it". Everything else here is a preference; this is a rule.

**Outward-facing artifacts name TIERS, never models.** Any harness-to-tier
mapping is a local deployment detail and stays out of published docs.

### 2.3 Dispatch gates

Per phase, in order:

| gate | do |
|---|---|
| concretize | restate the ask as one phase with a definite end |
| map | read the actual code; verify every claim the brief will assert |
| advise | ARCHITECT ruling on any open fork — no forks reach EXECUTE |
| brief | scope, seams, count sites, verification, hard constraints |
| execute | one agent, foreground, no `Monitor`, no backgrounded builds |
| review | different instance, re-derives from the diff |
| land | orchestrator reads the diff itself, then pathspec commit |
| log | commit message + wiki + memory delta |

**Briefs go stale silently.** Verify count sites, file paths, and line
numbers at brief time; do not copy them from memory. A brief that names a
site that moved sends the executor to rewrite the wrong thing.

**Ask the executor to report what the brief got wrong.** It is the most
valuable line in any report and it is routinely there.

### 2.4 Parallelism and the shared worktree

- Batch independent agents; keep conclusions, not file dumps.
- **Serialize anything that runs cargo.** Concurrent cargo on a shared
  `target/` produces flaky counts that read as real failures.
- Resume a stalled agent by id rather than respawning cold.
- Other sessions share this worktree. `git status --short` before dispatch;
  bounded paths per agent; never overlap writer paths.

---

## 3. The User's standing preferences

**Build for longevity, not for the fastest green.** Judge a design by
maintenance cost and reach. Between two working designs, pick the one that
deletes more.

**Critique before building.** If an idea is bad, say why *first*, then
build it if the User reaffirms. An objection after the fact is worthless.

**Ask often; one question per turn — the blocking one.** The User is vague
on purpose, to surface edge cases. Ambiguity gets a question, not an option
menu; menus only when a fork genuinely blocks. Unanswered twice → take the
default and flag it.

**Show, don't describe.** Processes, architecture, and data flow get ASCII,
tables, or trees. Prose fills only the gaps a picture cannot.

**Comments minimal — fix the code instead.** A comment justifies its
existence or it does not ship. Never brief an agent to match a heavy
comment density.

**Docs are timeless.** Edit a page integrally so it reads as always-so.
Never append dated UPDATE/AMENDMENT blocks. Decisions and change records go
to the log — commit message, changelog, memory. Ledgers are exempt; they
*are* the log.

**Say commands, not verbs.** Spoken as `aoide <cmd>` / `lyra <cmd>`.

**Backend first.** Doors, policies, protocol, crates before QML/rice/web.
In-flight front-end finishes; new front-end waits.

**Menus over hand-typed input.** Use the `inquire` wrappers in
`protocol/src/pick.rs` (`choose`, `choose_many`, `confirm`, `text_input`,
`hidden_input`). `inquire` is declared in `aoide-protocol` only — every
other crate reaches the wrappers. Keep the `*_reading` non-tty half so the
command stays agent-drivable and testable. **Ask the User which menu type
each time.** Carve-out: never convert an input to a menu where the typing
*is* the verification — a pairing SAS must stay typed, because selecting is
not attesting.

**Version bumps by phase.** One patch digit at the end of each completed
major phase, inside that phase's landing commit — never a commit of its
own. `pkgs/aoide/Cargo.toml`'s `[workspace.package].version` is the single
source: every crate inherits it, the runtime `AOIDE_VERSION` derives from
it via `env!("CARGO_PKG_VERSION")`, and the nix derivation reads it back
out of the manifest. Let cargo rewrite `Cargo.lock`. "Major phase" means a
numbered phase of a tracked lane, not every commit and not a review pass.

**Verify or say you could not.** "Looks done" is not done. Report failures
with the output; say plainly when a step was skipped.

---

## 4. Build, load, show

```
# eval + build (validates all nix)
nix build .#nixosConfigurations.<host>.config.system.build.toplevel --no-link

# activate — USER-GATED. Only when the User has admitted it.
sudo nix-env -p /nix/var/nix/profiles/system --set <toplevel>
sudo <toplevel>/bin/switch-to-configuration switch

# desktop-only reloads (no switch needed)
hyprctl reload                                     # compositor rules
systemctl --user restart aoide-quickshell.service  # bar / dock / gadgets
lyra rice stage <song>                             # livery hot-reload
qs -p modules/facets/quickshell/qml/shell.qml      # QML parse check

# show
grim out.png ; grim -g "0,0 1920x60" bar.png       # read them back yourself
```

- **Switch is the User's gate** ([[Rebuild-Gate]]). Never hold or use a
  password, even when handed one. Say the gate exists once, then move on.
- **Smallest reload that proves the change.** QML → quickshell restart;
  compositor rule → `hyprctl reload`; Stylix/base16 → needs a rebuild.
- **Staging can be locked.** `rice stage` refuses while
  `lyra rice mode status` reports `declarative`. Unlock with
  `lyra rice mode stage [<song>]`. See [[Self-Ricing]].
- **Never hand-edit** `song/stage/livery.json` or draft targets — that
  bypasses the lock check and the draft routing.

---

## 5. Verification discipline

- **`nix flake check` is the committed-tree gate.** `aoide soundcheck` is
  the working-tree sweep, report-only. Both exist; run them.
- **Flake checks see unstaged edits to TRACKED files, and are blind to
  UNTRACKED ones.** A new `.nix` file is invisible until `git add`ed —
  check `git status` before trusting a green.
- **Prove a refactor inert by drv identity.** Compare
  `nixosConfigurations.<host>.config.system.build.toplevel.drvPath` across
  a detached worktree. Pass the worktree as a **path literal**, never an
  interpolated string — a string leaks the temp path and fakes a
  difference. `pkgs/aoide/default.nix` can never pass this test: its
  `src = lib.cleanSource ./.` contains the recipe itself.
- **Prove a gate can go red.** A gate never seen failing is theatre.
- **Never `cargo fmt` or `rustfmt`.** No Rust fmt gate exists; HEAD is
  deliberately not fmt-clean and a run manufactures hundreds of churn
  lines. `nixfmt` on `.nix` files is required — the `fmt` check enforces it.
- **Never `cargo test --workspace`** — it deadlocks, since the conduct
  crate binds real sockets. Scope to changed crates, with `TMPDIR=/tmp`.
- **Read `PoisonError` as a cascade, not a cause.** Find the first failure.

---

## 6. Repo and git

Active development, not branch-protected. Small, coherent, verified changes
land directly.

- **Ask before branching.** Large, risky, or churn-heavy → ask. Default:
  land directly.
- **Pathspec commits only** (`git commit -- <paths>`). Never `git add -A`,
  never `git reset`/`checkout`/`restore`/`stash` in a shared worktree.
- **No AI co-author trailer.** Push as the repo's configured author.
- **Authored content says "the User"** — never a personal name.
- **Never commit** `.claude/settings.json`, `node_modules`, or
  `song/stage/*` / `state/stage/*` (live desktop state).
- **Push at milestones** — lane or phase completion, or on ask. Commit
  freely.

---

## 7. Wiki

Delegated to a standing execute-tier **librarian**, kept out of the
orchestrator's context. Mandate: every page states **what currently IS the
case**, present-indicative, matching the live repo. Never a plan or a
future intent.

- Update the page that owns the concept, not a new page.
- **Rice design → `song/songbook/<name>/design/`, not the wiki.** The wiki
  keeps only protocol ([[Ricing-Protocol]]).
- Follow [[Wiki-Protocol]] and `SCHEMA.md`: wikilinks, frontmatter, voice.
- Record the *why*, not only the *what*.

---

## 8. Live state lives in memory, not here

This page is timeless. Everything that changes — the task list, open flags,
in-flight lanes, per-agent quirks, deploy state, blocked items — lives in
the orchestrator's memory store and is read at session start.

| want | read |
|---|---|
| task list, lane state, blocking gates | the task-stack memory |
| what changed since last session | the newest handoff memory |
| per-agent quirks and dead ends | the per-agent memories |

A flag raised in conversation goes to memory the same turn. Nothing raised
is quietly lost; a parked flag is reopened after the detour that parked it.

---

## 9. Quick reference

| need | command |
|---|---|
| command ground truth | `aoide schema --json` |
| tier map at runtime | `aoide guide` |
| working-tree sweep | `aoide soundcheck` |
| committed-tree gate | `nix flake check` |
| conduct a session | `aoide conduct -- <cmd>` |
| command a session | `aoide send --id <id> [--submit] [--yes] -- <text>` |
| session roster | `aoide session` |
| mesh roster | `aoide peer list` |

## Related

[[Overview]] · [[Loop-Protocol]] · [[Agent-Interface]] · [[Session-Graph]] ·
[[Conductor-Channel]] · [[Rebuild-Gate]] · [[Self-Ricing]] ·
[[Ricing-Protocol]] · [[Wiki-Protocol]]
