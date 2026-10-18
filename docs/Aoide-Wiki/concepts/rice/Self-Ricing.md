---
type: concept
created: 2026-07-25
updated: 2026-09-26
tags: [aoide, rice, agent]
source: "[[references/AOIDE-HANDOFF]]"
---

# Self-Ricing — the Rice Loop

Aoide ships the rice engine as a builtin. The engine provides the loop, the schema, and the staging mechanism. Everything else — the rices, the preferences, the accumulated taste — it learns by doing.

**Status today:** the loop below is real end to end, `rice declare` included. `rice lint` (runs the native [[theme]] engine), `rice stage`, `rice compose`, `strx reload` (the one mode-aware iteration command — absorbed `quickshell reload` outright), the `rice draft` group (`save`/`list`/`drop`), and the `rice mode` group (`status`/`stage`/`declarative`/`draft`) are all implemented. `rice stage` stages `stage/theme.json`, [[Quickshell]] hot-reloads it live via `FileView`, geometry, window-border colours and the hyprglass switch apply to the running compositor over `hyprctl` in the same step, and the terminals follow: `stage/terminal-colors.conf` for every new kitty window, and a `kitty @ load-config` per instance for the open ones — while `rice mode declarative` is locked (below), `rice stage` refuses instead of writing. Beyond a live `rice stage`, the activation seed reseeds `stage/theme.json` from the active rice's committed notes with the venue's `aoide.theme.override` and the host's geometry applied and publishes the same bytes at `/declared/theme.json`, the declared twin that `rice mode declarative` re-pins from, beside `/declared/venue.json`, the slots the venue recolours and the geometry the host sets, as data ([[Codebase#Runtime contracts (socket + stage files)]]) — so a host that boots without ever staging still carries the correct stage twin, and a later re-stage of the declared rice reads the runtime rices, where the rice is edited, with the venue laid over it: the venue wins on the slots it recolours and the geometry it sets, and an edit shows everywhere else. The seed is declared truth and nothing more: the strillix lane's `aoide-rice-reload` unit then runs `strx reload` at every login (before the shell starts) and on every switch, which brings back whatever `stage/mode.json` names — the last staged rice, or the routed draft. `rice declare` is implemented — gated, it copies the runtime rice tree, less its top-level `history/` and `drafts/` (the machine's undo history and scratch, which stay behind), into the checkout's `rices/<name>/` (byte-diff, a repeat with nothing new is a no-op; no `git add`, no rebuild — the user runs those herself). `rice transpose` remains the one declared-but-not-implemented stub (exit `64`) — narrate it as planned, not as a working pipeline. There is no `rice gen`; `rice compose` is the real, working scaffolding entry point.

## The Rice Loop

```
strx rice compose <name> [--from <rice>]   (real — scaffolds a new rice, below)
    ↓  copies palette/window/geometry from --from (default "sonata")
strx rice mode stage <name>                (real — unlocks staging, stages declared content)
    ↓  edit the rice's files (hand or agent)
strx reload                                 (real — THE iteration command: reads
    ↓                                          stage/mode.json and reloads accordingly;
    ↓                                          staging snapshots the current rice —
    ↓                                          deduped against the head take, so an
    ↓                                          unchanged reload mints nothing — syncs it
    ↓                                          live, and reloads Quickshell, all in one
    ↓                                          call; loop this with edits freely)
rice lint                                    (real — theme schema validation)
    ↓  fail → reject + rices note
strx rice mode draft <draft-name>           (real — ROUTES stage/theme.json into a saved
    ↓                                          draft via a symlink; forks it from the
    ↓                                          current stage if new, and takes a baseline
    ↓                                          take of it; see Drafts below)
    ⋯ iterate freely: edit → `strx reload` → look, no separate save step; every
       reload snapshots the routed draft too (undo for free via `rice back`, back to
       the entry baseline at the least); switch
       to a different saved iteration any time with another `rice mode draft <name>` ⋯
strx rice mode declarative                  (real — tears the routing down, re-pins
    ↓                                          the declared truth; the draft itself
    ↓                                          stays saved on disk)
strx rice declare <name>                    (real — copies the rice, less its
    ↓                                          history/ and drafts/, into the
    ↓                                          checkout's rices/<name>/)
strx rice declare <name> --host             (real — rewrites the host config's one
    ↓                                          habit.rice.declared line to <name>;
    ↓                                          the RICE menu's Declare row runs it)
    ↓  gated rebuild (the user runs it herself)
```

The two declare steps are the two halves of "make this the declarative truth":
the first puts the rice's files where the build reads them, the second points
the host at it. `--host` takes a rice this build already carries
(`builtin.json`'s `rices` — the declared and available rices), never a draft,
and only a host file holding exactly one `habit.rice.declared = "<name>";`
line; it rewrites the string between the quotes and nothing else, so the next
rebuild performs that rice. It says so in its answer, and says too when the
staged copy holds theme or widget edits the build's copy lacks
(`stagedDiffers`): the rebuild builds the rices version, not the live
edits, so those reach the build through the first step and a commit.

Staging is the sketch — a live compositor call, no rebuild; `aoide.rice` selecting a rice and rebuilding is the truth — it bakes `hyprland.conf` (geometry, borders), themes every nix-manageable app via [[Stylix]], assembles and deploys the rice's widgets, and sets the host's boot default. GTK/Qt surfaces require app restarts and are declare-only, accepted by design.

## Composing a rice

Every runtime path on this page — `/stage/`, `run/qml/`, the composed
`rices/<name>/` and its `drafts/` — hangs off ONE runtime root:
`$AOIDE_ROOT` when set to an absolute path, else `~/.aoide` (on first run
the binaries move any pre-existing trees into it via
`fs::migrate_root_once()`). The git checkout is a separate thing, reached
through `$AOIDE_FLAKE_ROOT` (default `~/Aoide`): it holds the COMMITTED
rices at `rices/`, and `rice declare` is the seam that copies a
composed rice from the runtime root into it, less its top-level `history/` and
`drafts/`, which stay behind.

`strx rice compose <name> [--from <rice>] [--force] [--json]` scaffolds a new rice directly into the runtime rices — the starting point of the rice loop above:
`rices/<name>/rice.nix` (a body, which habit imports only when its rice is performed, CONTRACTS §5; its `system.module` copies the `palette`/`window`/`geometry` tiers from
`--from`, defaulting to `sonata` — the only `.nix` file the scaffold
writes, satisfying the rice-shape check), a `theme.json` mirror of the
same values, an honest-empty `design/intent.md` pointing at the widget-slot
catalog and the update playbook rather than fabricating design rationale,
and `widgets/.gitkeep`. No `hypr/` directory — geometry lives in `rice.nix`
alongside palette and window, not as a separate tier of files. Because it is
an ordinary schema command (`gated: false`, in `schema --json` and the MCP
tool list), an agent can bootstrap a rice through the same door a human
would.

The `--from` rice resolves from the host rices first, else from the
shipped score templates at `<templates>/<from>/theme.json`, where
templates is `$STRILLIX_TEMPLATES` (absolute) else
`<exe_dir>/../share/strillix/rices` when that directory exists.
`pkgs/strillix-rices` bakes the committed `rices/` tree plus
prebaked `manifest.json`/`registry.json`/`builtin.json` into that share dir,
and beside it the OFFLINE generator (`nix/manifest.nix` + copies of
`lib/rices.nix`/`lib/rice.nix` + the locked nixpkgs `lib/`) and
`aoide-options.json`. So a repo-less host composes from `sonata` with no
nix at all, and regenerates the widget registry/manifest over its OWN
rices with a plain `nix-instantiate --eval` on that shipped file — no
flake, no network, no checkout. A rice with a `_widgets/` shelf needs that
generator, not a checkout: `composeRice` runs inside it.

The same templates dir also seeds the runtime rices itself: the first
`strx rice stage <name>` / `strx rice mode stage <name>` for a rice that is
SHIPPED but was never composed on this host — `rices/<name>/` missing
outright, not just missing a file — copies that rice's whole template tree
in before staging proceeds, once, never touching a rices dir that
already has anything for the rice, even partially. This is what gives a
fresh host's very first `rice mode stage sonata` somewhere to read from and
write back to, without requiring `rice compose` first.

After the seed, the repo's changes to a built-in rice reach the machine through
`strx rice refresh [<name>]`: a file the machine never edited takes the repo's
version, an edited or unrecorded file is kept and reported, a file the machine
deleted is not re-added, and `history/`, `drafts/` and machine-only files are
never touched. `--check` reports each file's state and writes nothing. The rule,
the record it reads (`/declared/rices/<rice>.json`) and the first-run
behaviour are in `CONTRACTS.md` §5, "The rices belongs to the machine".

## Drafts — durable scratch, reached by ROUTING not copying

Between staging a rice and declaring it, drafts give the rice loop a way to
keep more than one live iteration around without committing any of them. A
draft is a saved snapshot living at
`rices/<rice>/drafts/<draft-name>/theme.json` (+ `wallpaper.json` when
the stage had one at save time) — nested under the rice it varies, not a
flat top-level dir: a draft is a variation of an already-composed rice, so
it belongs inside that rice's own directory, which the mechanism enforces
for free (a rice can only ever be staged, and therefore have a resolvable
"current rice," once it already exists under `rices/`).

**A draft forks the DRESS, not the widgets.** `theme.json` + `wallpaper.json`
are the whole of what a draft snapshots — widget QML bodies
(`rices/<rice>/widgets/*.qml`) are RICE-scoped, never
draft-scoped, because nothing routes them through the `stage/theme.json`
symlink the way palette/wallpaper values are routed. Practically: editing a
widget file while routed into ANY draft of that rice mutates what EVERY
draft of that rice renders, not just the one currently live — there is no
per-draft widget fork to isolate the edit into. `strx reload`'s take (below)
still captures widget bodies alongside theme+wallpaper on every mint, in every
mode; it is the take store, not the draft mechanism, that remembers them.

Outside the git checkout entirely — the runtime root owns
`rices/*/drafts/`, same as `/stage/` — and runtime state by
construction: the `rice-runtime-untracked` check
(`lib/checks.nix`) fails if a `rice/` runtime dir is ever committed,
`rice declare` never carries a draft into the checkout, and a draft copied
there by hand lands under a gitignored `rices/*/drafts/`. A draft is
durable scratch, never committed or declared truth. That distinction from the
checkout's committed `rices/<name>/` files is the entire point.

**Reaching a draft is `rice mode draft <name>`'s job — a distinct THIRD
mode, not a heuristic.** `RiceMode` is `Staging | Declarative | Draft`.
Entering `Draft` mode points `stage/theme.json` at a SYMLINK into the
draft's own file (forking it from whatever's currently in the stage first,
if the name doesn't exist yet). From then on, every writer of the stage
file — `rice stage`, a hand-edit, Quickshell's own FileView reload —
transparently lands in the draft, with no writer aware that routing exists:
`aoide_storage::fs::atomic_write` resolves and writes
through a symlink at its destination rather than replacing it (POSIX
`rename()` would otherwise silently replace the symlink itself on the very
first write). One reader is aware: a FileView holds the inode it loaded, so
the shell re-reads `theme.json` on every `mode.json` change, and every
command that swaps the entry writes `mode.json` last. Routing, not guessing — no mtime comparison, no auto-prefer
heuristic; a draft is only ever live because something explicitly pointed
the stage at it.

- **`rice mode draft <name>`** — routes `stage/theme.json` into
  `rices/<rice>/drafts/<name>/theme.json` (`<rice>` auto-resolved off
  the current stage, same as `rice mode stage`'s no-arg form). Forks the
  draft from the current stage first if it's a new name. Sets the marker to
  `{mode: draft, rice, draft: <name>}`, then mints a baseline take of the
  draft as entered (none when it equals the head). Refuses while `rice mode
  declarative` is locked.
- **`rice draft save <name>`** — an INDEPENDENT command: explicitly forks
  whatever's currently live into a new or updated draft snapshot, without
  switching modes. Useful for preserving a second variant while still
  working in a first one, or saving a snapshot from plain `Staging` without
  ever entering `Draft` mode. Upserts (re-saving an existing name
  overwrites it, clearing a stale `wallpaper.json` the current stage no longer
  carries). Never touches `mode.json`.
- **`rice draft list [<rice>]`** — enumerates saved drafts: name, rice,
  saved-at, and whether each is the currently-loaded one (`mode.json`'s
  `draft` field, which is set if-and-only-if `mode == draft`). No arg walks
  every rice's drafts; `<rice>` scopes to just that rice's.
- **`rice draft drop <name>`** — deletes a draft outright. Missing name is
  an error (not idempotent-silent). Refuses if it's the draft `Draft` mode
  currently has the stage routed to — switch modes first
  (`rice mode stage`/`rice mode declarative`, both of which tear the
  routing down), then drop it; a `drop` that also silently changed your
  mode would be the more surprising behavior.

**Leaving `Draft` mode:** `rice mode stage`/`rice mode declarative` both
tear the routing symlink down FIRST (removing it, so their own
declared-content write creates a real file) before doing anything else —
neither ever leaves a dangling symlink behind. Both `rice stage <name>` and
`rice mode stage` ALWAYS mean plain declared content now — no
draft-awareness of any kind, no auto-prefer heuristic. (Bare `rice stage`
with no name still auto-resolves the current rice, the same convenience
`rice mode stage`'s own no-arg form has — that's independently useful and
unrelated to drafts.)

**The round-trip this exists for:**

```
rice mode stage sonata          → stages declared sonata; marker {staging, sonata, ∅}
(hand-edit stage/theme.json)
rice draft save neon-night      → forks the edit into a NEW draft; rices/sonata
                                   itself untouched; marker unchanged (still staging)
rice mode declarative           → tears down nothing (wasn't routed), re-pins the
                                   DECLARED rice, locks; the draft stays on disk
rice mode stage                 → unlocks, re-stages declared sonata — PLAIN declared
                                   content, no auto-prefer; marker {staging, sonata, ∅}
rice mode draft neon-night      → ROUTES stage/theme.json into the saved draft (it
                                   already exists, so no fork); marker {draft, sonata,
                                   neon-night}
(edit stage/theme.json again)  → lands DIRECTLY in the draft file — no save step
rice mode declarative           → tears the routing down, re-pins declared; locks
rice mode stage                 → plain declared sonata again, no auto-prefer
rice mode draft neon-night      → routes back to the SAME draft file, still carrying
                                   every edit made while it was live
```

Multiple drafts coexist independently — saving `amber-dusk` alongside
`neon-night` never touches it, and `rice draft list` shows both.

## Staging vs Declarative Mode

`rice stage` and `wallpaper set` are the only two COMMAND writers of
`stage/theme.json`, and three commands write `stage/wallpaper.json`: `rice
stage`, `wallpaper set` and `rice back` (the lane's own
`home.activation.aoideSeedStage` reseeds `theme.json` on every activation and
never touches `wallpaper.json`) — unaffected by which of
`stage`/`declarative`/`draft` currently owns the routing, see below.

**Who PAINTS a pick is the host's choice, not a rice's.** The wallpaper PROVIDER
is a provider capability (`modules/dendrites/wallpaper/`) with two alternatives
today — `quickshell`, the shell's own `aoide-wallpaper` layer, which draws the
rice's own default and the user's picks, and `skwd-wall`, an external provider
that draws picks on its own surface. A host picks one with
`habit.dendrites.wallpaper.provider`, and the chosen provider file names
itself in the fact `aoide.wallpaper.provider`; `strillix` publishes that one word to
the runtime as `/stage/wallpaper-provider`, which both the CLI
(`wallpaper_provider::provider`) and `ThemeState.qml` read — absent means the
shell's own layer. The rice is not consulted: a pick is the same
`stage/wallpaper.json` either way, and the shell's layer simply paints nothing
while an external provider has one on screen. `strx wallpaper sync` is the bridge —
the staged pick applied while one applies, the step-aside image when none does —
and every pick write calls it.

`stage/mode.json` — a runtime stage-file under the root — records which of
**three** modes currently owns those writes (`RiceMode`:
`Staging | Declarative | Draft`):

- **`staging`** — hot-load unlocked; `rice stage`/`wallpaper set` write live,
  always as a plain real file.
- **`declarative`** — nix/home-manager is the only writer; `rice stage`/
  `wallpaper set` refuse outright with a `declarative-mode-locked` error naming
  `rice mode stage` as the way to unlock — except `wallpaper set --from-skwd`,
  which still records what an external wallpaper provider is already showing
  (the stage file must describe the screen, CONTRACTS.md §4). An absent
  `stage/mode.json` reads
  as `declarative` — the safe default, since nothing has ever unlocked
  staging writes.
- **`draft`** — `stage/theme.json` is a SYMLINK routed into a saved
  `rices/<rice>/drafts/<name>/theme.json` via `rice mode draft <name>`;
  see [[Self-Ricing#Drafts — durable scratch, reached by ROUTING not copying]]
  for the full mechanism. `rice stage`/`wallpaper set` still write normally —
  they carry zero awareness that routing exists.

**Staging is never declarative** (house rule 10). Everything a staged rice
brings live comes from the runtime root: the notes, the wallpaper, the widget
bodies, and the rice's slot-owner map and widget-type registry
(`run/qml/rices/manifest.json`/`registry.json`). None of it is evaluated
from the git checkout: the manifest/registry sources are the shipped
templates' baked files (for a rice this system built in) and this machine's
own rices, evaluated by the SHIPPED generator over
`$AOIDE_ROOT/rices`. So a rice that exists only in the runtime
rices stages and hot-loads without a commit, a merge or a rebuild. The
declarative path may seed staging (the activation seed, the baked
templates), never gate it. The staged rice is always the LAST one staged:
`stage/mode.json` remembers it as `stagingRice`, and every way back into
staging (the RICE menu's last-staged row, a bare `rice mode stage`, a login after a
reboot or a switch) restores that rice, never the declared one.

A switch and a reboot restore it the same way. The activation lays the
declared rice over the runtime tree — the rsync into `run/qml` and the seed
into both stage twins — and never reads the mode. Then `aoide-rice-reload`, a
user unit of the strillix lane, runs `strx reload` in the user manager, before the
shell: it re-stages the rice `stage/mode.json` names (palette, wallpaper, widget
bodies, slot map, registry, compositor keywords, terminal colours), or
re-routes and re-syncs the routed draft. At login the session starts the unit;
on a switch the activation restarts it together with the shell. So the shell's
first frame after either is the staged rice; in `declarative` the reload only
reloads the shell, the seed already being what shows.

The checkout is reached by exactly two staging-adjacent steps: `rice
declare`'s commit-in from the runtime rices, and `strx preview declare`'s
from a preview root. Nothing else touches it.

`rice stage` doesn't only hot-load the palette/notes tier any more —
it also syncs the rice's widget QML **bodies**
(the composed copy under `$AOIDE_ROOT/rices/<name>/widgets/*.qml`,
not the git checkout — an edit reaches the desktop only once it is placed
there) into the live runtime tree
(`run/qml/rices/<name>/`, `crate::widgets` in `crates/strillix-rice/`), so an edit to
an EXISTING widget file reaches the desktop through Quickshell's own
file-watcher, no rebuild. This rides the same mode gate as everything
else in this section — locked under `declarative`, allowed under
`staging`/`draft` — so there is no separate lock to reason about. A
brand-new widget file is the one thing this doesn't cover: `manifest.json`
is only read at Quickshell startup, so a new slot still needs a
`systemctl --user restart aoide-quickshell.service` to be discovered.
`strx reload`'s final beat, in every mode, is an unconditional Quickshell
IPC reload (rebuilds the whole scene fresh from `shell.qml` — the absorbed
`quickshell reload` mechanism, byte-for-byte in the `declarative` arm);
whether that also re-reads `manifest.json` and closes this gap is still
unconfirmed against a live instance.

The terminals and the compositor's glass follow the stage too, from the
same staged notes and under the same gate. `rice stage` writes
`$STRILLIX_ROOT/stage/terminal-colors.conf` — the staged rice's base16 in
kitty's own colour syntax (the tinted-kitty template Stylix bakes, key for
key; a rice with no base16 tier gets the scheme the stylix lane
synthesises from its palette, so every slot is written either way and no
slot of an earlier rice survives), through the theme engine's `kitty`
emitter, plus the rice's `geometry.terminalOpacity` as kitty's
`background_opacity`, written only for a rice that has one. The kitty
dendrite includes that file after Stylix's baked colours and after
`/declared/terminal-opacity.conf` — the one-line declared fragment the
activation seed writes from the same field (host settings included) and
deletes when it is null. A rice with no opinion therefore writes no opacity
line, and kitty falls through to the fragment and then to kitty.conf's own
`background_opacity`, the host's bake (0.86 on the kitty dendrite): a host
that has never staged anything still opens its terminal at the declared
rice's opacity, and the staged file, included LAST, wins when it carries one.
So every NEW window opens in the staged rice; the windows already open
follow when `rice stage` runs `kitty @ --to unix:<sock> load-config` over
every kitty control socket (`$XDG_RUNTIME_DIR/kitty-<pid>`, the dendrite's
`listen_on`) and reports how many windows it reloaded. With no path kitty
re-reads its own kitty.conf and every include, so an open window ends where
a new one opens, and each window's font zoom resets. Never a raw OSC write into a pty: that
interleaves with whatever the program there is printing and gets eaten. `rice
mode declarative` rewrites the file from the declared twin through the same
re-pin, so leaving staging restores the declared colours the same live way;
`rice back` and `strx reload`'s draft sync write it too. The lock screen
follows through the same writers: a rice that ships a `lockscreen.<ext>` at its
folder root dresses hyprlock with that picture and colours from tiers it
already authors, as hyprlang variables in `$STRILLIX_ROOT/stage/hyprlock.conf`
(the `hyprlock` emitter), and a rice that ships none leaves the lock as the host
has it. Nothing is pushed: hyprlock reads the file at every lock. On the compositor
side, `geometry.blurEnabled` switches hyprglass along with Hyprland's
blur: `plugin:hyprglass:enabled` and `plugin:hyprglass:layers:enabled` are
live keywords, so a rice with blur off (cadenza) turns the glass off without
unloading the plugin, and a rice with blur on turns it back on. The BAKE takes
the same two keys from the same field, so the compositor lane's
`hyprland.conf` reads glassless for cadenza too — booted and staged agree. A rice with
no `blurEnabled` opinion sends neither: the `hyprctl reload config-only` every
live stage runs first has already put the host's glass back, so a no-opinion
rice staged after cadenza gets its glass back.

**Polarity is the one theme field `rice stage` does not apply.** A rice's
`aoide.theme.polarity` (`"light"`/`"dark"`, beside its palette) is read by the
stylix lane alone and lands on the [[Self-Ricing#The Rice Loop|gated rebuild]],
the font tier's posture: GTK/Qt and every Stylix target take the register when
their theme is built, so no keyword, no file and no emitter call carries it —
`rice stage` neither applies it nor claims to, and a staged rice cannot flip a
live desktop's polarity. A rice's committed `theme.json` does carry it beside
`palette`, and the activation seed copies that document into
`/stage/theme.json` ([[theme#Two fan-outs, one source]]), so the staged
document may hold a `polarity` value as data for a future live reader; nothing
reads it today.

`strx rice mode status` reports the current mode plus, in `staging`/
`draft`, which rice (and, in `draft`, which draft) it is pointed at and
since when. `strx rice mode stage [<name>]` unlocks staging AND leaves
`draft` mode if currently in it (tearing the routing symlink down first);
given a name, it also stages that rice's declared content immediately,
combining unlock-and-stage into one call. `strx rice mode declarative
[<name>]` locks staging (also leaving `draft` mode the same way) — with a
name, or with none: it re-pins `stage/theme.json` to the resolved rice's
declared notes FIRST (with no name, the DECLARED rice — the rice
`/declared/theme.json` names, the venue's activation-published twin of
the committed notes with any `aoide.theme.override` already applied; only a
host that never activated the lane falls back to the current stage's own
rice, the auto-resolve `rice mode stage` uses) and only writes the lock
marker after that write succeeds, so the re-pin can never trip the lock it
is about to set. The re-pin reads the twin, not the runtime rices:
declarative mode is declared truth, so an edit to the runtime rice stays out
of it, where `rice stage` and `rice mode stage` show that edit with the venue
laid over it. This means a bare `rice mode declarative` **discards
whatever unsaved live edits sat in the stage** — `rice draft save` first to
keep them (the CLI's own success message says as much). Only when no rice
can be resolved at all (a genuinely fresh box with no stage file and no
declared twin) does it fall back to a bare lock with nothing to re-pin.

The enforcement lives at the two write entrypoints only —
`handle_rice_stage_entry` in `crates/strillix-rice/src/commands/rice.rs`,
`handle_wallpaper_set_entry` in `crates/strillix-rice/src/commands/wallpaper.rs` — with no
background reconciler watching `stage/mode.json`. `rice stage` and `wallpaper
set` are the only two places in the codebase that ever write those two
stage files, so guarding their entrypoints closes every path a drift could
take; [[aoided]] itself is still a one-shot skeleton with no event loop.
The routing symlink's own transparency lives one layer lower, in
`aoide_storage::fs::atomic_write` itself (resolves and writes through a
symlink at its destination rather than letting POSIX `rename()` replace
it) — general behavior, not draft-specific, since every stage-file writer
in the codebase routes through that one function.

## Geometry

A rice may set `aoide.theme.geometry` — gaps, border size, rounding, and
blur, every field optional — alongside its palette and window tiers; see
[[theme#The geometry tier]] for the field list and the fallback/live-apply
mechanism. A rice that sets no geometry performs with the host's baked
values, unchanged: every live stage reloads the compositor's config before it
lays the rice's own fields over it.

## The Shipped Baseline Is Guarded, Not Frozen

`sonata` is the shipped standard, guaranteed present under
`rices/sonata/`. `aoide.rice` (`nullOr str`) defaults to `null` —
naming no rice performs no rice: a host opts into the desktop by naming the
rice explicitly. A missing baseline is a loud nix eval failure, never a
silent no-op. `sonata` is upstream-owned and
evolving: like any other upstream-owned tree (nucleus, dendrites), upstream
MAY update or iterate on it.

Every OTHER rice — anything composed via `rice compose` under a name other
than `sonata` — upstream never touches, a guarantee unchanged by `sonata`
itself being both the shipped baseline and actively iterated. What protects
a rice from being clobbered is that nothing in this system overwrites
silently: `rice compose` without `--force` refuses to touch a rice that
already exists, no generator writes into a rice unprompted, and
`rice declare` (either step) is gated by design — the rebuild that follows it is the
user's own step.

## Rices Discipline — the "Self" in Self-Ricing

The agent reads `rices/` and the relevant rice's `design/` before iterating. It appends after every declare or reject. Without this write-back the system is a theme generator; with it the system accumulates taste.

- `rices/learnings.md` — cross-cutting observations.
- `rices/preferences.md` — accumulated from declare/reject decisions.
- `rices/update-playbook.md` — schema migration instructions.
- `rices/<rice>/design/` — per-rice design intent, palette rationale, iteration log.

Design folders are ordinary content-pipeline folders, pre-approved as system-owned. Aoide ingests its own design notes so the agent can query past rice reasoning like any other domain.

The discipline is already in use: the retired `default` rice's declared aesthetic — Windows-7-sidebar chrome with ASCII/box-drawing note theming, realized as the [[Gadget-Dock]] — is distilled into `rices/learnings.md` now that `default` itself is gone, so future generations still inherit that design decision as cross-cutting memory.

`rice lint` validates a rice against the note schema; a full rice missing an instrumented surface (e.g. the lockscreen) fails loudly.

## Declare, Select, Replay

**Declare** copies the runtime rice tree, less its top-level `history/` and
`drafts/` (the machine's undo history and scratch, which stay behind), into
the checkout's `rices/<rice>/` as durable, versioned, fleet-available
score — a gated byte-diff copy (a repeat with nothing new is a no-op), with
no `git add` and no rebuild: the user commits and runs the gated rebuild
herself. **`aoide.rice`** is the per-host
selector (`nullOr str`, default `null` — naming no rice performs no rice) —
one line in `hosts/<host>/default.nix`:

```nix
aoide.rice = "sonata";
```

**Replay** is performing a declared rice at a different host: the rice
carries only theme (palette + component tiers), the host supplies its own
specifics (hardware, monitors) and enabled instruments (dendrites).
A host lacking an instrument does not sound that part.

**Transpose** vs **replay**: transpose is same venue, new key (new
palette); replay is same score, new venue (different host). Both are
one-line operations on a declared rice — the replay declaration
(`aoide.rice = "<name>";`) is real, `rice transpose` itself is planned.

## Keys and Transposition

Each rice's `rices/<rice>/palette/` holds its transpose keys — the palette variants that rice can swap among. Wallpaper extraction emits new keys as standalone artifacts. `rice transpose <rice> <key>` (planned) replays a rice in another key. This is only possible because the semantic and component tiers never touch raw color values directly.

## Per-Rice Structure

```
rices/<rice>/
├── rice.nix      pure nix: theme import + config swaps (wallpaper note points at $STRILLIX_ROOT/wallpapers/<file>)
├── theme.json   theme values
├── palette/      transpose keys
├── sounds/       chimes / notification audio
├── icons/        per-rice icon overrides
├── widgets/      per-rice widget bodies
├── design/       design wiki: intent, palette rationale, log
└── drafts/       (runtime-only, outside the checkout) saved rice-draft scratch — see Drafts above
```

Wallpapers themselves live in the shared `/wallpapers/` library, not per-rice — any rice's `rice.nix` references a file there by literal path.

`rice/` is the agent's writable domain. The agent commits there and nowhere else. Declare = copy the runtime rice tree into the checkout, less its top-level `history/` and `drafts/` (the machine's undo history and scratch, which stay behind), then the user's own commit + gated rebuild.

## Related

- [[Rice-Vocabulary]]
- [[theme]]
- [[aoided]]
- [[Content-Pipeline]]
- [[Snowflake-Anatomy]]
- [[Stylix]]
- [[Gadget-Dock]]
