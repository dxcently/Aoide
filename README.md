# Aoide

**Aoide (the core) vs AoideOS (the distribution) — don't conflate the two.** *Aoide* is the **orchestration core**: the bridges and APIs between the terminal, the shell, the system, and the OS — one interface through which agents are freely orchestrated for any task. Any agent with a shell is fully capable, no MCP required; **every terminal is a conductable, tracked session by default**, so a central agent can speak into any other running session (see [Conducting](#conducting--every-terminal-is-a-tracked-session)). The core runs anywhere there is a shell — portable, headless-capable, agent-first. *AoideOS* is the **distribution built on that core**: the NixOS flake that ADDITIONALLY ships the Quickshell widget-making toolkit (bar, dock, gadgets, the DAG/conductor surfaces) and the specialized ricer (the song/notes theming engine). Aoide is the engine; AoideOS is the desktop around it — a capability that works with only a shell is "Aoide", one that is desktop/Quickshell/rice is "AoideOS". This README documents AoideOS (you're reading the flake), with the core's `aoide` verbs throughout.

**AoideOS** (what this flake is) is the agent-wearable NixOS desktop distribution — Hyprland compositor, Quickshell shell, an orchestrator daemon (`aoided`), a content pipeline, and a self-ricing engine — built on the shell-only Aoide core, that you **fork and run**, not install. Its naming thesis in one line: architecture is frozen music — the nix layer is the score, the running desktop is the performance, a rice is a song the system sings.

This README is a use guide: how to drive the box day to day, how to edit the flake, and what the `aoide` tools do. For the deeper why, see the wiki at [`docs/Aoide-Wiki`](docs/Aoide-Wiki/Overview.md) (start with `Overview.md`; the [[Wiki-Protocol]] note explains the shape).

> Status — walking skeleton. The desktop, the flake, the `graph` group, the daemon, and the theming fan-out are **real and running live** (yomi-strix is switched onto this flake). The `rice`, `content`, `make`, `update`, and `onboard` verbs are **structured stubs** that return exit `64` (`not-implemented`) with the right shape — the trunk is wired, the muscle is being grown. Stubs are marked honestly throughout.

---

## 1. Quick start — daily driving

### The `ad*` rebuild family

All of these are shell aliases (from the `bash` dendrite, `modules/dendrites/bash.nix`), pointed at the Aoide flake at `/home/khoa/Aoide`. They wrap [`nh`](https://github.com/viperML/nh) (nix-helper).

| Alias | Runs | What it does |
|---|---|---|
| `ad` | `cd /home/khoa/Aoide` | Jump to the flake dir. |
| `adrebuild` | `nh os switch` | Build + activate + set as boot default (the everyday rebuild). |
| `adupdate` | `nh os switch --update` | Same, but bump flake inputs first (`flake.lock`). |
| `adboot` | `nh os boot` | Build + set as boot default, activate on next reboot. |
| `adtest` | `nh os test` | Build + activate **without** touching the boot default (reverts on reboot). |
| `adbuild` | `nh os build` | Build only — no activation, no privilege needed. |
| `adrollback` | `nh os rollback` | Roll back to the previous generation. |
| `adcheck` | `nix flake check` | Run the flake's `checks` (coupling, song-shape, packages, VM boot). |
| `adgens` | `nh os info` | List system generations. |
| `adclean` | `nh clean all` | Garbage-collect old generations + store paths. |

Typical loop: `adbuild` to validate → `adtest` to try it live → `adrebuild` to make it stick. The rebuild is the **user-gated** step (see [Rebuild Gate](#the-rebuild-gate)); agents propose, you type the switch.

### Desktop controls

Compositor keybinds (`modules/facets/compositor/default.nix`). The `SUPER` key is the modifier.

| Keybind | Action |
|---|---|
| `SUPER + SPACE` | Toggle the launcher (`aoide shell launcher toggle`). |
| `SUPER + G` | Toggle the **gadget dock** — open-and-pin the left-edge popup (`aoide shell dock toggle`). |
| `SUPER + L` | Lock the screen (`aoide shell lock`). |
| `SUPER SHIFT + P` | `aoide rice preview` — rehearse the current rice live. |
| `SUPER SHIFT + A` | `aoide rice adopt` — commit the previewed rice (user-gated). |
| `SUPER + RETURN` | Open a terminal (kitty). |
| `SUPER + Q` | Close the active window. |
| `SUPER + V` / `SUPER + F` | Toggle floating / fullscreen. |
| `SUPER + arrows` (or `H/J/K`) | Move focus (arrows: full set; H/J/K: vim left/down/up — right is arrow-only, `SUPER + L` stays lock). |
| `SUPER SHIFT + arrows` (or `H/J/K`) | Move the window. |
| `SUPER ALT + arrows` (or `H/J/K`) | Resize the window (repeats while held). |
| `SUPER + 1–0` | Switch to workspace 1–10. |
| `SUPER SHIFT + 1–0` | Move window to workspace 1–10. |
| `ALT + Tab` | Previous workspace. |
| `SUPER + X` / `SUPER + Z` | Toggle special workspace `magic` / `scratch` (`SHIFT` = move window there). |
| `SUPER + leftdrag` / `rightdrag` | Move / resize window with the mouse. |

> The `aoide shell *` verbs the keybinds call are not yet in the command schema — they are a documented open seam (the bridge path is stubbed). The hot-edge and pure-QML paths work today regardless. dxflake's media/brightness hardware keys are deliberately unbound — the bar's volume cell owns audio by mouse.

**Gadget dock** (`SUPER + G` or hover the left screen edge): a Windows-7-sidebar-homage popup — box-drawing chrome over Aero-glass blur, all colour from notes. Slides in on hot-edge hover, pins via the `[+]/[■]` header affordance. Holds the terminal roster, a compact session DAG, clock, meters, now-playing, power, and calendar gadgets. A row click in the terminal roster jumps straight to that terminal's window — the click issues a socket command to `shellbridge` (`{cmd:"focuswindow",address}`), which dispatches `hyprctl focuswindow`; QML never shells out.

**Bar cells** (`modules/facets/quickshell/qml/AoideBar.qml`) — the bar is one bar of music: a cream frosted-glass "manuscript sheet" (0.58 opacity, close to the terminal's own glass — see below) with a five-line staff. Workspaces are **solid musical note glyphs** (♩ ♪ ♫ ♬…), one distinct shape per workspace id, rising in pitch with it; the active workspace's note swells and fills with the song's accent colour, urgent ones pulse. The bar's popouts (now-playing, volume, battery, calendar) share the same cream glass. Real Quickshell interactivity throughout:

| Cell | Interaction |
|---|---|
| Clock (center) | Click → anchored `CalendarGadget` month-grid popup. |
| Volume (right) | Scroll = adjust, click = mute, hover = slider. |
| Sessions (`✎`, left) | Click → toggle the gadget dock. |
| Battery / network | Hover popout; note-glyph icons. |

### Look & feel — the current song

The desktop ships keyed to the **`sonata`** song (`song/songbook/sonata/`): a
LIGHT warm classical-academic palette (`stylix.polarity = "light"`) drawn by
hand from its wallpaper, Alma-Tadema's *Unconscious Rivals* — cream/parchment
base, deep umber ink, dusty-cornflower accent, sage-green "hot" trace colour,
muted-rose urgent. The terminal (kitty, Aero-glass over the compositor blur,
0.60 opacity) and the bar (the cream manuscript sheet above, 0.58 opacity)
are tuned close together so they read as one continuous surface rather than
two panes that happen to share a hue — see the wiki's [Pantheon
Grammar](docs/Aoide-Wiki/design/Pantheon-Grammar.md) for the full visual
grammar and [the Ricing Protocol](docs/Aoide-Wiki/design/Ricing-Protocol.md)
for the house rule this rework introduced: one base16 file drives every
surface via Stylix, and any rice/song change gets a vision-check that
terminals and widgets still agree on light/dark and colour.

---

## 2. The flake, instructionally

The repo is a **snowflake**: everything lives under `modules/`, walked and self-registered by an in-house dendritic walker (`lib/walk.nix` + `lib/mkHost.nix`). No import lists — drop a `.nix` file in the right directory and it registers itself. A `/_`-prefixed path (`_wip.nix`, `_scratch/`) is **hidden** from the walker.

```
~/Aoide/
├── modules/
│   ├── nucleus/    core: aoided daemon, shellbridge, CLI packaging, options, policy
│   ├── dendrites/  opt-in features — one tree, shipped + personal branches
│   └── facets/     render surfaces (read ONLY aoide.drachma): quickshell · compositor · stylix
├── hosts/
│   ├── common/     cross-machine baseline (which dendrites default ON)
│   └── <host>/     machine-specific picks (hardware, enabled facets, song)
├── pkgs/           aoide CLI (Rust) · drachma (Node)
├── lib/            the walker + mkHost + checks
├── song/           the performed half (rices, songbook, runtime stage/)
└── flake.nix       inputs + outputs (never edited to add a module)
```

### Enable / disable a dendrite

One line in a host (or `hosts/common/default.nix` for the whole fleet):

```nix
aoide.obsidian.enable = true;   # turn a shipped dendrite on
aoide.btop.enable = false;      # opt a common-default dendrite off for this host
```

### Change the song (theme)

One line in `hosts/<host>/default.nix` — the whole notes fan-out swaps, zero other edits:

```nix
aoide.song = "moonlight";       # default = the shipped standard
```

Songs are **host-agnostic score**: any host performs any committed song by naming it. Songs self-register from `song/songbook/<name>/rice.nix` on the same walker principle.

### Add a new host

Add one entry to `flake.nix`'s `nixosConfigurations`:

```nix
nixosConfigurations.<host> = mkHost "<host>";
```

then create `hosts/<host>/default.nix` importing `../common` and flipping its flags (copy `hosts/yomi-strix/default.nix` as the template). A committed `hardware.nix` is imported guardedly if present.

### Add your own dendrite

The five-line recipe (from `modules/dendrites/_example.nix`, dendrite shape v0 — `CONTRACTS.md §2`). Drop this at `modules/dendrites/<name>.nix`:

```nix
{ config, lib, ... }:
{
  options.aoide.<name>.enable = lib.mkEnableOption "<name>";
  config = lib.mkIf config.aoide.<name>.enable {
    # carry your own dependencies; read no other module (only config.aoide.* you declare)
  };
}
```

Then enable it with one host line. Growth is additive — new dendrites are new files, so upstream merges stay conflict-free. Rules: guard on `aoide.<name>.enable`; a dendrite never reads another module; facets are the same shape but MAY read `aoide.drachma`.

**Where things belong** (the walker discovers all four; subfolders are grouping only):

| Directory | Put here | Who may change it |
|---|---|---|
| `modules/nucleus/` | daemon, CLI packaging, the option contract, policy | upstream merge only |
| `modules/dendrites/` | opt-in features (yours + shipped) | additive — new files freely |
| `modules/facets/` | render surfaces (read only `aoide.drachma`) | upstream merge only |
| `song/songbook/` | committed songs (the agent's writable domain) | agent, gated at rebuild |

### The fork-and-run model

Aoide is a framework you **fork**, not a package. Fork upstream → `git clone <your-fork> ~/Aoide` → `aoide onboard` seeds the fork and prints the guide (the eventual one-shot install; `onboard` is a stub today). Shared git history makes `aoide update` a real `git merge`, so upstream improvements flow in and your personal branches never conflict. **Today**, drive it with `nixos-rebuild` / `nh` against the flake (the `ad*` aliases above).

---

## 3. How Aoide updates

Two axes move independently. **Dependency versions** are yours to bump on any schedule; the **framework itself** is upstream's shape, pulled in by merge. No background updater touches either — house policy, both go through the gated rebuild.

### Dependency updates — routine, yours

Bump the flake inputs (nixpkgs, home-manager, stylix, quickshell, hyprland, nvf) and rebuild in one step:

```
adupdate            # = nh os switch <flake> --update  → rewrites flake.lock, then switches
```

This only moves pinned dependency versions; the frozen machinery is untouched. See the [`ad*` table](#the-ad-rebuild-family) in §1.

### Framework updates — upstream's shape, merged in

The nucleus, lib, and facets are upstream's to evolve. Fork-and-run keeps shared git history, so a framework update is a real merge, not a package swap:

```
git fetch upstream && git merge upstream/main    # improvements flow in; additive growth stays conflict-free
adcheck && adrebuild                             # run the flake's checks, then switch
```

`aoide update` is the eventual guided path for this — fetch upstream, merge framework paths, run `checks`, detect contract bumps (`CONTRACTS.md`) and route them through `song/songbook/update-playbook.md`, then propose the gated rebuild. **Stub today** — exit `64` (`not-implemented`); use the two-line `git` form above meanwhile.

### Who owns what

Ownership follows radial distance from the nucleus (the snowflake's [mutation policy](docs/Aoide-Wiki/concepts/Snowflake-Anatomy.md#mutation-policy)) — not directory fences. It is tracked by git merge-base, and divergence from inherited files is warned, not blocked.

| Layer | Owner | You do this |
|---|---|---|
| `modules/nucleus/` + `lib/` + `CONTRACTS.md` | upstream | Don't edit — merge cleanly. |
| `modules/facets/` | upstream | Toggle per host; render surfaces are upstream's. |
| `modules/dendrites/` (shipped) | upstream ships | Toggle via `enable` flags. |
| `modules/dendrites/<yours>.nix` | you | Grow new branches freely (new files). |
| `hosts/` | you, entirely | Hardware, enabled facets/dendrites, song pick. |
| `song/songbook/` | you (agent-written) | Where self-ricing writes back its songs; gated at rebuild. |
| `pkgs/aoide` + `pkgs/drachma` | upstream | Don't edit — merge cleanly. |

### Merge hygiene

Your edits live in `hosts/`, `song/`, and **new** dendrite files — all additive, so upstream merges stay conflict-free by construction. Editing a nucleus, facet, or lib file is how you earn conflicts on the next merge; when you need a change there, PR it upstream instead of forking the shape.

---

## 4. The aoide tools

### `aoide` — the CLI trunk

`aoide <cmd>` is the **complete** capability surface; any agent with a shell is fully capable. Every command takes/emits `--json`, returns structured exit codes, and is idempotent. `aoide schema --json` is the machine-readable single source of truth (42 commands), and the MCP tool list is **generated from it** — one implementation, three doors, no drift. Run `aoide guide` for the tier-0 onboarding.

The 42 commands, grouped (`real` = implemented; `stub` = exit `64`, not-implemented):

- **Orientation** — `guide`, `schema`, `mcp serve` — all `real`.
- **`rice` group** (the self-ricing loop): `rice lint` `real`; `rice gen`, `rice preview`, `rice adopt` (gated), `rice transpose` — `stub`.
- **`content` group** (the discover→approve→ingest pipeline): `content register`, `content propose`, `content approve` (gated), `content ingest`, `content query` — all `stub`.
- **`graph` group** (the session/project DAG, all `real`): `graph view`, `graph project add/remove/list`, `graph link`, `graph focus`, `graph prune`, `graph emit`, the session write-verbs (`graph session start/phase/end/hook`), and the wrappers `graph wrap` (observe-only) + `graph send` (the gated injection door).
- **`conduct`** (`real`): run a command on its own PTY as a **conductable** session (a control socket `graph send` can type into). Conduct-by-default makes this every terminal's login shell (see [Conducting](#conducting--every-terminal-is-a-tracked-session)).
- **`conductor`** (the interactive terminal UI): `real` — an interactive, ASCII-art terminal UI over the same trunk (see below). `aoide conductor` (the TUI, a noun) is distinct from `aoide conduct` (a verb) which wraps a single process into the conductor channel.
- **Daemon runners** — `daemon` (aoided), `shellbridge`, `adapter melete` — all `real` skeletons.
- **Lifecycle** — `make` (widget-maker), `update` (gated self-update), `onboard` (first-boot) — all `stub`.
- **`a2a` group** (the A2A/Agent2Agent door, CONTRACTS.md §6): `a2a serve` — `real` (AgentCard + `tasks/get` + `message/send`, inject-or-spawn); `a2a agent add/list/remove` — `stub` (the client-side registry is a later phase).

### `aoide mcp serve --stdio`

The stdio MCP server. Off by default (`aoide.mcp.enable = false`); agents spawn it per-session. Its tools derive from `schema --json`, so the MCP and CLI doors cannot diverge. Network MCP (the dedicated "Aoide connector") is **user-enabled only**, never by an agent.

### `aoide conductor` — the interactive terminal UI

An interactive terminal UI over the trunk, built to **conduct the running agent
sessions and jump between them** — an active conductor, not a dashboard. It is a
strict frontend: every action is a `Door::Cli` dispatch through the one
dispatcher, so a conductor action is audited exactly like a typed command, and reads
reuse the `graph` pure functions. Four panels (`1`–`4`/`Tab`): **[1] SESSIONS** —
the hero panel, the live DAG roster grouped under **collapsible project headers**
(`h`/`l` fold/unfold, `[live/total]` badge), each session row a musical state
glyph (♪ working · 𝄐 awaiting · 𝄽 idle · 𝄂 done), agent, cwd, elapsed clock and
parent chain, with fresh arrivals accent-marked (`j`/`k` select, `Enter` jump to
the window, `L` link a session under a parent, `a`/`d` add/remove a project, `p`
prune, `e` emit); **[2] PROJECTS** (`a` add, `d` remove); **[3] LOG** — the audit
tail; **[4] STATUS** — stage-tree health. `?` for help, `q` to quit. The chrome is
the gadget dock's: box-drawing frames, `[▓░]` meters, and the staff-run ornaments
lifted verbatim from the Quickshell QML. It honours `$AOIDE_STAGE_DIR` /
`$AOIDE_AUDIT_LOG`, so a throwaway tempdir is a full rig — try it without a live
desktop in one line:

```sh
export AOIDE_STAGE_DIR=$(mktemp -d) AOIDE_AUDIT_LOG=$AOIDE_STAGE_DIR/log
pkgs/aoide/tests/fixtures/seed.sh "$AOIDE_STAGE_DIR" && aoide conductor
```

### Conducting — every terminal is a tracked session

Conduct-by-default is the Aoide **core** behaviour (works with only a shell; `concepts/Conductor-Channel`). **Every kitty window runs its login shell under `aoide conduct`**, so each terminal is its own tracked, conductable session: it registers in the session DAG AND holds a per-session control socket (`$XDG_RUNTIME_DIR/aoide/session-<id>.sock`) that a central controller can type into. The wiring is one line in the `kitty` dendrite — kitty's `shell` points at an `aoide-shell` wrapper — and it is written to be **unbreakable**: if `aoide` is missing from PATH, or `AOIDE_NO_CONDUCT=1` is set (the escape hatch), or the conduct fails to exec, the wrapper falls straight through to the plain login shell. Nested terminals spawned from a conducted shell parent themselves into the DAG via the inherited `AOIDE_SESSION_ID`.

`aoide conductor` (the TUI, a noun) is distinct from `aoide conduct` (a verb) which wraps a single process into the conductor channel.

To command another session:

```
aoide graph send --id <id> [--submit] [--yes] -- <text>
```

It injects `<text>` into that session's stdin (`--submit` appends Enter). This is the **one gated injection door**: held **pending** approval by default; it **delivers** on `--yes`, on the global `AOIDE_CONDUCT_AUTOGATE` switch, or when the **sender is the target's parent** (an orchestrator freely commanding a child it spawned — the "freely orchestrated" default). A delivered send auto-renames the node to a one-line form of the text, and **every outcome is audited** (`~/Aoide/log`). `aoide conduct -- <cmd>` wraps any extra agent the same way; `aoide graph wrap -- <cmd>` is the lighter observe-only wrapper (no control socket). The desktop's terminals become a mesh of sessions a conductor speaks into.

### `drachma` — the note engine

The Node package (`pkgs/drachma`, wrapping Style Dictionary — the token engine takes the Greek coin's name) that owns the authoritative note pipeline: **lint** (validate a rice against the note schema — `rice lint` delegates here), **resolve** (apply component→palette fallbacks), and **emit** (write the resolved `song/stage/drachma.json` for Quickshell, plus hyprctl and terminal-OSC targets). Notes are the single immutable seam between the frozen nix layer and the live desktop — facets read `aoide.drachma` and nothing else.

### `aoided` + `shellbridge` — the runtime services

Both run as **user services** on the desktop:

- **`aoided`** — the orchestrator daemon: a neutral event stream, the single policy surface, lint, and one audit log (`~/Aoide/log`). Every operation through either door flows through it.
- **`shellbridge`** — the daemon-to-desktop bridge: publishes session/hook roster state to `song/stage/{sessions,hooks}.json` atomically, consumes Hyprland IPC. Its unix-socket accept loop is live (`aoide shellbridge --run`) — a `{cmd:"focuswindow",address}` line drives `hyprctl dispatch focuswindow`, the click-to-jump verb the gadget dock's terminal roster calls. This is the seam the terminal-roster gadget and the bar sessions cell read. A companion systemd timer (`aoide graph reap`, ~12s) sweeps sessions killed uncatchably (SIGKILL/`SUPER+Q`) so the roster never accumulates dead rows.

### Melete / Mneme integration

Aoide **integrates and launches** — it does not vendor — two independently-owned systems: the **Melete** coding harness (the "doer": autonomous runs, shell, GitHub, fleet, scheduling; `melete aoide …` routes *back into* this CLI trunk, i.e. Melete can drive Aoide) and the **Mneme** knowledge server (the "door": the vault MCP API behind the content pipeline and wiki protocol). What ships in *this* repo is the **seam**, not their code: `modules/dendrites/{melete,mneme}.nix` launch each self-updating runtime binary, and the `melete-adapter` (`modules/nucleus/melete-adapter.nix`) translates neutral `aoided` events into Melete job dispatches under a default-deny subscription — treating forwarded notification text as untrusted **data**, never a command. The Aoide·Melete·Mneme (three-Muses) naming is a *theme*, not a claim they are one program. Many "Aoide features" (fleet, scheduling, messaging) are Melete capabilities surfaced through Aoide's CLI/widgets.

### The Rebuild Gate

The one carefully-gated step in the loop (`adopt` → commit → **rebuild**). **Default: agents propose, you switch.** An agent can `build`, `dry-activate`, and `test` freely (no privilege), but to make a change stick it stops and prompts you to run the `switch` under your own `sudo`. NixOS rollback backs every switch (the old generation stays bootable).

Opt-in: the **`aoide.rebuild`** capability (off by default) grants a dedicated no-login agent user a **passwordless but narrowly-scoped** path — a polkit rule that lets it `systemctl start` a fixed `aoide-rebuild-{test,switch}.service` unit and nothing else, with the flake path/host/verb baked in. Chosen over storing a sudo password because security comes from scope + recoverability + audit, not a typed secret. It changes only *authentication* — the approval gate on `switch` persists (no background rebuilds).

---

## 5. Full command reference

### Rebuild aliases (`bash` dendrite)

`ad` · `adrebuild` · `adupdate` · `adboot` · `adtest` · `adbuild` · `adrollback` · `adcheck` · `adgens` · `adclean` — see [§1](#the-ad-rebuild-family).

### `aoide` subcommands (from `schema --json`, 28)

| Command | Status | Summary |
|---|---|---|
| `aoide guide` | real | Print the four-tier agent onboarding (tier map + house rules). |
| `aoide schema` | real | Emit the versioned machine-readable schema of every command + state file. |
| `aoide rice gen` | stub | Generate a rice from a prompt or wallpaper (reads `songbook/` first). |
| `aoide rice lint` | real | Validate a rice against the note schema (delegates to `drachma`). |
| `aoide rice preview` | stub | Rehearse a rice live (`stage/drachma.json` hot-reload); nothing committed. |
| `aoide rice adopt` | stub · gated | Commit a previewed rice and propose the gated rebuild. |
| `aoide rice transpose` | stub | Replay a song in another key (palette) from the song's `songbook/<song>/palette/`. |
| `aoide content register` | stub | Register a content source folder (points in place; never copies). |
| `aoide content propose` | stub | Propose a discovered source for admission through the approve gate. |
| `aoide content approve` | stub · gated | Admit a proposed source (the user admits; non-negotiable gate). |
| `aoide content ingest` | stub | Index an approved source in place, then lint (fail → quarantine). |
| `aoide content query` | stub | Query the content index. |
| `aoide make` | stub | Widget-maker: generate a dendrite + widget + adapter from an intent. |
| `aoide update` | stub · gated | Fetch upstream, merge framework paths, run checks, propose the rebuild. |
| `aoide onboard` | stub | First-boot flow: register the fork, seed songbook, print the guide. |
| `aoide mcp serve` | real | Run the stdio MCP server; its tool list is generated from this schema. |
| `aoide daemon` | real | Run the `aoided` daemon skeleton (policy, lint, gate, single audit log). |
| `aoide shellbridge` | real | Publish session/hook state to `song/stage/` atomically. |
| `aoide graph view` | real | Render the project/session DAG (Unicode tree; `--json` emits the document). |
| `aoide graph project add` | real | Register/update a project anchor root (atomic, idempotent). |
| `aoide graph project remove` | real | Unregister a project anchor root (ok + no-op if absent). |
| `aoide graph project list` | real | List the registered project anchor roots. |
| `aoide graph link` | real | Record a spawned-by edge (set `parentSessionId`; cycle-checked). |
| `aoide graph focus` | real | Jump to a session's window via `hyprctl focuswindow`. |
| `aoide graph prune` | real | Drop `done` sessions + hook records; clear orphaned links. |
| `aoide graph emit` | real | Stage the resolved DAG to `song/stage/graph.json` (atomic). |
| `aoide graph wrap` | real | Run any agent command as a registered session (inherited stdio, observe-only); exports `AOIDE_SESSION_ID`, exit mirrors the child. |
| `aoide graph send` | real | Inject text into a conducted session's control socket — the one gated injection door (pending by default; `--yes`/autogate/parent-of-target delivers; every outcome audited). |
| `aoide conduct` | real | Run a command on its own PTY as a **conductable** session (control socket + best-effort `windowAddress`). Every kitty terminal runs its shell under this by default. |
| `aoide conductor` | real | Raise the conductor: the interactive terminal UI to conduct the agent sessions — session DAG, projects, audit log, stage status (every action routes through the same dispatcher). |
| `aoide adapter melete` | real | Run the melete-adapter: consume the neutral event stream (default-deny). |

### Shell QoL aliases (`bash` dendrite)

| Alias | Runs |
|---|---|
| `v`, `nv` | `nvim` |
| `lg` | `lazygit` |
| `crc` | `claude --rc` |
| `y` | `yazi` cd-wrapper (chdir to where yazi last browsed) |
| `ls` / `ll` / `la` / `lt` | `lsd` / `lsd -l` / `lsd -la` / `lsd --tree` |
| `..` | `cd ..` |
| `reboot` / `shutdown` / `poweroff` | `systemctl reboot` / `poweroff` / `poweroff` |
| `sleep` / `hibernate` | `systemctl suspend` / `hibernate` |
| `lock` | `hyprlock` |

### Desktop keybinds

Aoide workflows: `SUPER+SPACE` launcher · `SUPER+G` gadget dock · `SUPER+L` lock · `SUPER SHIFT+P` rice preview · `SUPER SHIFT+A` rice adopt.

Window management (ported from dxflake): `SUPER+RETURN` terminal (kitty) · `SUPER+Q` close · `SUPER+V` floating · `SUPER+F` fullscreen · `SUPER+arrows` (or `H/J/K`) focus · `SUPER SHIFT+arrows` (or `H/J/K`) move window · `SUPER ALT+arrows` (or `H/J/K`) resize · `SUPER+1–0` switch to workspace 1–10 · `SUPER SHIFT+1–0` move window to workspace 1–10 · `ALT+Tab` previous workspace · `SUPER+X`/`SUPER+Z` special workspace `magic`/`scratch` (`SHIFT` = move there) · `SUPER+leftdrag`/`rightdrag` mouse move/resize — see [§1](#desktop-controls).

---

## 6. Dendrite & facet roster

### Facets (3) — the desktop surfaces, enabled per host

| Facet | Enable flag | What it is |
|---|---|---|
| Quickshell | `aoide.facets.quickshell.enable` | The QML shell surfaces (bar, dock, launcher, OSD, lock, greeter, wallpaper, graph). |
| Compositor | `aoide.facets.compositor.enable` | The Hyprland compositor + all hyprctl-level keybind wiring. |
| Stylix | `aoide.facets.stylix.enable` | Base16 baked-theme fan-out from `aoide.drachma` to every nix-manageable target. |

### Dendrites (20) — opt-in features

Fifteen default **ON** for the whole fleet via `hosts/common/default.nix` (the dev-tool baseline plus Melete/Mneme); the remaining five ship **OFF** and are opted in per host. Any common-default can be flipped off with a per-host `aoide.<name>.enable = false;`.

| Dendrite | Enable flag | Default | What it is |
|---|---|---|---|
| bash | `aoide.bash.enable` | ON | Interactive bash: the `ad*`/QoL aliases, fastfetch greeting, tty1→Hyprland hand-off. |
| nh | `aoide.nh.enable` | ON | `nh` (nix-helper) rebuild wrapper pointed at the Aoide flake. |
| git | `aoide.git.enable` | ON | git + gh for the aoide user (LFS, identity, credential helper). |
| kitty | `aoide.kitty.enable` | ON | The Kitty terminal (colours/fonts deferred to Stylix). |
| neovim | `aoide.neovim.enable` | ON | Neovim via nvf (rose-pine, LSP/treesitter, telescope, neo-tree). |
| starship | `aoide.starship.enable` | ON | The Starship prompt (musical-notation theme). |
| mcfly | `aoide.mcfly.enable` | ON | Smart shell history (vim keys, fuzzy search). |
| btop | `aoide.btop.enable` | ON | The btop resource monitor. |
| yazi | `aoide.yazi.enable` | ON | The yazi terminal file manager. |
| fastfetch | `aoide.fastfetch.enable` | ON | The fastfetch greeting (bundled musical ASCII logo). |
| devtools | `aoide.devtools.enable` | ON | CLI dev-tool toolbox — dev-specific only (neovide, lazygit, claude-code, nix tooling, nodejs, ngrok). |
| cli | `aoide.cli.enable` | ON | General CLI utilities (fzf, htop, fd, ripgrep, jq, ffmpeg, archives, curl/wget, file, vim, …). |
| fonts | `aoide.fonts.enable` | ON | System font set (musical notation, CJK, emoji, nerd fonts). |
| melete | `aoide.melete.enable` | ON | The Melete AI harness service (inactive until its binary is deployed). |
| mneme | `aoide.mneme.enable` | ON | The Mneme vault MCP server (inactive until its config is deployed). |
| hyprland | `aoide.hyprland.enable` | OFF | Host-invariant Hyprland behaviour — keybinds, input devices, tiling layout, misc, behavioural window rules. Pairs with the compositor facet, which owns the look. |
| obsidian | `aoide.obsidian.enable` | OFF | Obsidian knowledge-base integration (vault watcher + bar widget). |
| firefox | `aoide.firefox.enable` | OFF | The Firefox browser (colours/theme deferred to the Stylix facet). |
| screenshot | `aoide.screenshot.enable` | OFF | Human screen capture (hyprshot + satty; ships the SUPER+S binds). |
| vision | `aoide.vision.enable` | OFF | Agent screen-capture primitives (grim + slurp on PATH). |

`modules/dendrites/_example.nix` is the shelved template (walker-hidden by its `_` prefix).

---

*Contracts and state-file formats are versioned in `CONTRACTS.md`; module-authoring lives in `docs/BUILD.md`; the house rules and agent tiers are in `AGENTS.md`.*
