# Aoide operations handoff — 2026-09-14

This is an operational snapshot for the next orchestrator. It distinguishes
source completion, agent-reported tests, integration and deployment. The older
`AOIDE-HANDOFF.md` is historical design material, not the current operational
contract.

## Mission and priorities

1. Continue Conductor development and open the actual updated binary in a
   visible terminal for the user.
2. Deliver a clean, pushed dxflake checkpoint that Osaka and Sakaki can rebuild.
3. Integrate reviewed mail reliability fixes; repair Sonata source freshness and
   stage/declared state retention.
4. Desktop preview is **parked by the user**. Preserve its branch and evidence;
   do not continue experiments or make it a release dependency.

Aoide core is the portable orchestration engine (`aoide`/`aoided`). Lyra owns
rice, QML and desktop preview. dxflake is the first live consumer of portable
Aoide/Lyra flake exports. Migrate AoideOS's own architecture after that portability
is proven, not by assuming dxflake's migration also migrated AoideOS.

## Ownership and execution

Opus/Fable designs, writes bounded briefs, reviews and integrates. Sonnet executes
substantive implementation. Keep one owner per slice and avoid overlapping
writers. Resume existing agents rather than spawning duplicate orchestrators.

| Lane | Worktree / branch | Claude session | Mailbox |
|---|---|---|---|
| dxflake | `/home/khoa/worktrees/dxflake-fable`, `fable/dxflake-architecture` | `dc07e0be-7355-4f5c-a8a1-ce957197cc1d` | `dxflake-opus` |
| Conductor | `/home/khoa/worktrees/aoide-conductor-scene`, `codex/conductor-scene` | `618a4724-a01a-47f8-b10a-3423c0e95037` | `opus-conductor-scene` |
| Mail reliability | `/home/khoa/worktrees/aoide-mail-reliability`, `codex/mail-reliability` | `0aab5e6a-2d76-489f-8a9a-50863c32affe` | `opus-mail-reliability` |
| Fable integration / Sonata | `/home/khoa/Aoide` | `f8febccb-d47b-44a7-b6a4-ee8a148a21cb` | `claude-mail` |
| Desktop preview, parked | `/home/khoa/worktrees/aoide-desktop-preview`, `codex/lyra-desktop-preview` | `68d729fb-af20-417e-8187-e810de27bbaf` | `opus-desktop-preview` |

Root's mailbox is `codex-integration`. IDs are a snapshot: check liveness before
sending or resuming. Background state labels alone are not proof of progress.

## Verified source state and reported acceptance

### dxflake

Local tip checked: **`94295ac`**. Owner reports it pushed to
`origin/fable/dxflake-architecture`; main was not the push target. Local branch
still tracks `origin/main`, so do not blindly use bare `git pull` as a handoff
instruction. Fetch and explicitly select the architecture branch after checking
the destination checkout for local edits.

| Commit | Result |
|---|---|
| `7cb4a5c` | Nested Hyprland session environment guard |
| `7aed1e8` | Hyprland ecosystem split into smaller dendrites |
| `9216b2d` | Common package lists moved beside aggregation imports |
| `4d31d5d` | Transience rice extracted without changing rendered derivations |
| `762f56d` | `songbook/transience/palette.nix` becomes the palette source |
| `94295ac` | Hyprland aggregation renamed to `compositor` |

Owner reports all four system toplevels realised successfully, sequentially:
Osaka, Sakaki, Chiyo and Yomi. Build execution used an isolated `4d31d5d`
checkout; all four derivation paths were verified identical at `762f56d`.
The subsequent rename was reported parity-preserving; recheck its exact
derivations before treating the current tip as the final rebuild checkpoint.
No activation or boot verification occurred.

Transience has one Nix palette source. `livery.json` remains generated and checked
for drift. Stylix consumes the palette. The old Waybar appearance was preserved;
Rofi and Wlogout obtain their styling through Stylix. Wallpaper remains shared.
The full Lyra compose/stage/declare loop does **not** support this non-QML rice.

The dock is not selected by Transience: Osaka's Aoide Quickshell facet supplies
it independently. This is an explicit limitation against the user's request to
include the widget dock; do not describe palette inheritance as a dock dependency.
Osaka still has hand-written `aoide.livery.override` values, some intentional.
Do not flatten those differences while removing duplication.

Architecture authority: [Nix composition](../../architecture/NIX-COMPOSITION.md).
Separate aggregation directories, singular `aggregation` options, selective
imports, shared users and provider selections remain the intended shape.
Override records can target dendrites and hosts and carry overlays/configuration;
aggregation-targeted overrides were discussed but are not confirmed implemented.
Two shelved `_` dendrites retain obsolete option names; these are cleanup, not
rebuild blockers.

### Conductor

Local tip checked: **`e7f86dd`**, following retained-scene commit `d950a84`.
Owner reports 170 passing tests and 18 follow-up commits. Not confirmed integrated
into the root branch or installed system binary.

- Top-down tree, retained node positions, fixed world-space cards and camera.
- Navigation follows the tree axes; right-click resolves the intended node.
- Popup geometry uses display-cell measurements shared by draw and hit testing.
- Project/mail scrollbars; graph cards use Ratatui Block and Buffer primitives.
- Redundant harness fallback text removed from untitled cards.
- Owner reports real-terminal screenshots proving tree shape, pointer menu
  anchoring, right-edge clamping and clipped-card rendering.

Two windows titled `aoide conductor` were visible at the last root check. Their
binary provenance was not independently established. Verify the process executable
and build revision before claiming the updated UI is open. Unset inherited
`NO_COLOR` when launching it.

Remaining: node dragging, initial framing of wide trees, live Ctrl-wheel proof,
and overflow scrollbar proof. Keep thin box-drawing wires unless the user changes
that preference. No new decision is needed merely to retain the current wires.
The graph CLI still has old DAG wording outside the Conductor crate, and the task
register needs an evidence-backed refresh.

### Mail reliability

Local tip checked: **`71a10cb`**, following `57fdafc`, based on root `99c144a`.

- `57fdafc`: refused entries no longer consume the drain batch; failed atomic
  writes remove temporary files and stale dead-process temps can be reclaimed.
- `71a10cb`: transport backoff survives a persistence failure; ring writes have
  timeouts so a non-reading peer cannot indefinitely hold the ring lock.
- Remaining: connect-side bound in the ring critical section; full-spool parsing
  still occurs under the shared lock. Review before calling the channel bounded.

The reported `mail send --thread` hang was not reproduced. A held shared
`song/stage/.stage.lock` demonstrably blocks mail without output; the original
holder was not identified. Do not claim finite probes exonerate every thread path.

SUN_LEN failures were traced to long test temporary paths in interactive
`nix develop`. Owner reran with `TMPDIR=/tmp`: conduct 773, client 294, server 216
passing. Shorten the test-support helper in a separately owned fix; do not wave
off future socket failures without checking their cause.

### Sonata and mode lifecycle — unresolved

Fable has the assignment but no fresh completion report. Local runtime and mutable
songbook Conductor QML hashes differed from current repository source. Osaka used
an older Aoide pin and raw cream seed despite a Rose Pine host override.

Both stage and declarative paths called the mutable songbook staging loader;
the mode marker retained a song name, not necessarily the edited candidate bytes.
Required acceptance:

```text
edited stage QML + palette
        -> declared deployed source
        -> stage restores the exact edited candidate
```

Preserve drafts and candidate files before changing this lifecycle. Verify the
public flake export, deployed seed, runtime QML and palette together. A successful
build alone does not establish that Osaka is displaying current Sonata.

### Desktop preview — parked

Preserve **`88e3016`** and
[preview proposal](../../architecture/LYRA-DESKTOP-PREVIEW.md).
The concept came from a nested-compositor bug, not an intentional terminal renderer.
Owner reports live nested rendering, isolated input, resize and both teardown
paths. An earlier containment claim used a stale binary and was explicitly
corrected; use only the rebuilt-binary evidence.

No private session bus, one output, no full panel embedding, and rice surfaces
can clip after resize. The preview reads real state but daemon writes are
dead-ended. A global song reaper may kill unrelated Quickshell children; this
production isolation concern remains relevant even while preview work is parked.
Never kill a compositor based on the historical PID in an old letter: verify its
environment and shutdown hooks first.

## Resume procedure

1. Read root `AGENTS.md`, relevant crate instructions and the composition document.
2. Inspect worktree status and mail before assigning work. Root remains at
   `99c144a`; completed side branches are not automatically integrated.
3. Review the mail and Conductor commits, run focused tests, resolve integration
   without sweeping unrelated staged files into a commit.
4. Open the verified Conductor binary for the user. Keep production services and
   existing sessions alive.
5. Confirm dxflake remote tip and exact derivation parity, then provide the user
   the explicit branch/revision and host rebuild command. User activates.
6. Obtain Fable's Sonata report, or explicitly resume its existing session with
   that assignment. Mail filing alone is insufficient.

```sh
aoide mail read --for codex-integration --json
# Recovery only: --reread includes already-read letters.
aoide mail send --from codex-integration --to self/<mailbox> \
  --subject 'Concrete task and acceptance' -- 'Bounded assignment'
claude agents --json
claude logs <short-id>
```

Root last attempted `aoide send --id <full-id> --yes --submit` to all five owners:
each failed with **no control socket**. Three background owners were then stopped
while idle and resumed using full session UUIDs; the CLI confirmed the same
sessions woke. Do not create a resume copy while an owner is still running.
Fable's interactive session was mailed, not confirmed woken.

## Guardrails and pending local material

- No unattended system activation. Build-only checks are allowed; user rebuilds.
- Run builds sequentially and tests per crate, not `cargo test --workspace`.
- Preserve logs, drafts and queued mail; no mass queue flush or broad process kill.
- Root has unrelated staged Obsidian, host and inference-module changes. Never
  use a blanket commit or reset to integrate these branches.
- Root also has an uncommitted Stylix cursor change: `pkgs.maplestory-cursor`,
  name `Maple`, size 40. Review and integrate within its own scope.
- Composition, HTTPS mesh, mail-only access and preview architecture documents
  are untracked in the root checkout. Preserve them. HTTPS mesh remains deferred.
- No claim of current stage/declared correctness, automatic doorbell readiness,
  remote activation success or complete Aoide/Lyra portability is justified yet.

Evidence: local branch/status checks and mailbox sequences 786, 788, 798–800,
810, 814–818 in `~/.aoide/state/mail/base.jsonl`. Agent reports are identified above;
repeat live checks where deployment or process state matters.
