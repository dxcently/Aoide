# AOIDE — design handoff

Status: design settled in chat, nothing built. This doc is the contract for the
build and for the rice co-design session on yomi-strix (User + Claude, on-box).
Read fully before writing any nix.

---

## 1. What Aoide is

Aoide is an agent-agnostic graphical body for an AI agent: a NixOS
framework you fork and run — upstream is the source, your fork is your
instance, self-updating from upstream and self-configuring to your
preferences. Upstream ships the shape-making, never the shapes: the
rice-shaping framework, its contracts, the shipped standard, and the
management tools — users grow their own branches. It turns a machine into a
desktop an agent can wear — Hyprland +
Quickshell shell, an orchestrator daemon, a content pipeline, and a
self-ricing engine as the headline feature. Melete is the resident agent that
wears it; any agent with a shell can. Naming: the third Boeotian Muse — Melete
(practice), Mneme (memory), Aoide (song). Song orchestrates.

The naming thesis, engraved: architecture is frozen music. The nix layer is
the score — crystalline, immutable, snowflake-shaped like Nix's own logo —
and the running desktop is the performance. A rice is a song the system
sings; self-ricing means Aoide composes its own. Snowflake vocabulary (§2)
names the frozen half, song vocabulary (§3) names the performed half, and
tokens are where they meet: notes frozen into the crystal, sounded at
runtime. The fork lives at `~/Aoide`; the performed half lives inside it
as `Song/` — crystal outside, song within; onboard links `~/Song` →
`~/Aoide/Song` so the English name is real.

```
            agent (swappable — claude cli primary)
               │ shell always · mcp optional
┌──────────────▼───────────────────────────────┐
│ ~/Aoide — YOUR FORK of the framework         │
│  aoided        orchestrator: events·policy   │
│  shellbridge   json state · sockets · hypr   │
│  quickshell    bar·notifs·widgets·launcher   │
│  rice engine   gen·lint·preview·adopt        │
│  compositor    hyprland module               │
│  integrations  mneme · obsidian(toggle,off)  │
└──────────────┬───────────────────────────────┘
               ▼
┌──────────────────────────────────────────────┐
│ Song/ — the performed half, in the fork      │
│ repertoire/ keys/ covers/ chimes/ songbook/  │
│ runtime, gitignored: stage/ backstage/ …     │
└──────────────────────────────────────────────┘
   ~/Melete  ~/Mneme  ~/Magi — peers, never owned
```

## 2. Settled decisions (one line + reason each)

Ship shape
- A framework you fork: clone your fork to `~/Aoide` + `aoide onboard` is
  the whole install — shared history with upstream, so `aoide update` is a
  real git merge and improvements can flow back; module import into another
  flake stays possible but secondary.
- Reproducibility is the point of nix and the fork honors it: the default
  rice and every generated one are versioned — a rebuild from the fork
  reproduces the entire riced system anywhere; gitignored runtime holds
  ephemera only, nothing reproduction needs.
- Upstream scope is deliberately thin: the rice-shaping framework (engine,
  contracts, walker, facets), the shipped standard, and the management
  tools — not a curated integration garden; users grow their own branches
  and figure out their own shapes.
- One crystal, one dendrite tree: personal branches (flatpak.nix,
  firefox.nix, jellyfin.nix …) grow in the same `dendrites/` as shipped
  ones — ownership is provenance (git merge-base), not directory fences;
  growth is additive, so upstream merges stay conflict-free by
  construction.
- Core stability is contractual, not polite: token schema, dendrite shape,
  `schema --json`, and stage file formats are versioned interfaces
  (CONTRACTS.md in core); `checks/` fails a merge that breaks one, and
  `aoide update` detects contract bumps and routes them through the
  playbook instead of the rebuild discovering them.
- Composition engine is the in-house dendritic walker — den dropped (pattern
  prior art only): every file under a walked dir self-registers; snowflake
  anatomy names the layers.
- Naming quirk (snowflake morphology): nucleus = the core everything
  condenses around; dendrites = branching opt-in features; facets = render
  surfaces catching the same tokens in different light; rime = the deposited
  aesthetic layer (rice engine + shipped default).
- Why the snowflake fits: Nix's logo is literally a snowflake, so the repo
  names its anatomy after its own substrate; crystals grow from a nucleus by
  local accretion — exactly how dendritic self-registration grows the flake —
  and no two crystals are alike: same physics (shared flake), unique hosts.
- The metaphor encodes the mutation policy: radial distance from the nucleus
  = change frequency = who may change it — modifying inherited structure:
  upstream merge · growing new branches: one host line to enable · Song/:
  agent + User's gate.
- Songs are pure nix in the fork: `Song/repertoire/` is a repo path,
  committed and enabled per host — no flake-input hack, purity for free,
  adopt is a commit plus the gated rebuild.
- Package scopes — narrowest wins, merging is native nix: a dendrite
  carries its own dependencies (enable shell, get quickshell) ·
  hosts/common/ holds the cross-machine baseline + overlays + unfree ·
  hosts/<host>/ holds machine-specific (ROCm on yomi-strix, not chiyo) · a
  song may carry fonts/cursors. Graduation rule: a bare package lives in a
  list, a package with opinions becomes a dendrite. `aoide pkg add <p>
  --host|--common|--song` writes the right list and reports the diff.
- Self-update: `aoide update` fetches upstream, merges framework paths,
  runs checks, then proposes the gated rebuild — the fork updates itself,
  the gate still decides. No background updaters, house policy.
- Shipped `rime/default/` carries the default rice + wallpaper in the same
  per-rice folder shape as generated ones — one contract, two locations,
  shipped side immutable.
- Tokens are a standalone package (§4) — the immutable data core; nix is the
  mutable structure around it, `rice.nix` is the joint between the two.

Intended repo layout

```
~/Aoide/ — your fork    U upstream-grown · Y your growth (provenance, not fences)
├── flake.nix            U  inputs: nixpkgs · home-manager · stylix
├── nucleus/             U  core: aoided · shellbridge · policy · cli
├── dendrites/          U+Y ALL branches, one tree: shell · compositor ·
│                           agents · integrations · mcp (off) · your
│                           growth: flatpak · firefox · jellyfin …
├── facets/              U  quickshell (live shell — bar · notifs ·
│                           launcher · osd · lock · greeter · wallpaper) ·
│                           stylix (baked — gtk/qt · terminal · editors ·
│                           browser · boot) · compositor (hyprctl) — each
│                           reads tokens, nothing else
├── rime/                U  rice engine + default/ (the standard)
├── lib/ checks/ docs/   U  walker · lint · CONTRACTS.md · AGENTS.md
├── pkgs/                U  aoide cli · token package (until it graduates)
├── templates/           U  the forkable template itself
├── hosts/
│   ├── common/          Y  baseline pkgs · overlays · unfree
│   └── <host>/          Y  picks branches, one line each · host pkgs
└── Song/                Y  the performed half, whole
    ├── repertoire/<song>/  rice.nix · tokens.json · liner/ · assets/
    ├── songbook/           learnings · preferences · playbook
    ├── keys/ covers/ chimes/   palettes · wallpapers · sounds
    └── stage/ backstage/ auditions/ catalog/ index/ log/
                            runtime, gitignored
```

- `hosts/` knows dendrites; dendrites never know hosts — dxflake separation,
  verbatim; subfolders are grouping only, the walker registers every file.
- Facets are the only nix that renders appearance, and they read only the
  tokens option — the coupling discipline survives den's exit.
- Optional commit-to-the-bit renames: hosts → crystals, lib → lattice.
- Outputs: `nixosConfigurations.template`, `packages.aoide`,
  `templates.default`, `checks`.

Agent interface
- CLI is the trunk: `aoide <cmd>` (with `melete aoide …` passthrough) is the
  full capability surface; any agent with a shell is fully capable.
- MCP is a façade generated from the same command schema — one
  implementation, two doors, no drift.
- MCP is optional and agent-added: `mcp.enable = false` default; agents that
  want it spawn `aoide mcp serve --stdio` per session; network MCP
  (tailnet/funnel) is enabled by User only, never by an agent.
- Guide tiers: 0 `aoide guide` / AGENTS.md at a well-known path (herdr
  agent-guide pattern) → 1 CLI → 2 stdio MCP → 3 network. `aoide schema
  --json` is the machine-readable backstop; MCP tool list generates from it.
- Agent-first ergonomics everywhere: every command takes and emits `--json`,
  errors are structured with meaningful exit codes, every state file
  (stage/, tokens, manifests) has a published schema, all operations are
  idempotent and report what changed — the CLI is an API that happens to be
  typeable.
- Primary agent is claude CLI: spawn wrapper registers the session and window
  address with shellbridge, Claude Code hooks (Notification/Stop) post state;
  other agents get the wrapper or degraded process-signal states.
- yomi-strix note: the box runs local inference (Ollama, ROCm on gfx1151) —
  the agent-agnostic CLI means rice gen can run against local models offline;
  claude CLI remains the first-class path.

Desktop
- Hyprland is the compositor and the only multiplexer — no PTY layer.
- Quickshell is the shell: bar (workspaces + agent sessions + connection
  state), notif daemon (org.freedesktop.Notifications native), agent widgets,
  launcher, OSD. Rice references: unixporn canon (quickshell/ags bars,
  swaync-style centers, anyrun launchers) — patterns only, styling comes from
  tokens.
- shellbridge bridges everything: atomic JSON state files out, unix socket
  commands in, Hyprland IPC consumed — no MCP in QML, ever.
- Session jump: widget click → shellbridge socket → `hyprctl dispatch
  focuswindow address:…`.
- Event path: aoided emits a neutral event stream; thin per-agent adapters
  translate (melete-adapter → job dispatch); subscriptions are default-deny
  per event class so OSD noise cannot burn agent runs.
- Forwarded notification text is untrusted input — adapters wrap it as data;
  an app title must never command the agent.

Content
- Pipeline: discover → propose → approve → ingest → lint → query, with a
  quarantine branch on lint failure; drift re-lints and quarantines rather
  than poisoning the index.
- Folder contract: plain files + `.aoide/manifest.toml` (kind + conventions
  profile); index points at content, never copies.
- The approve gate is non-negotiable: silent adoption of arbitrary folders
  into an agentic index is an injection surface — discovery finds, User
  admits.
- Mneme sources are read via Mneme's API only, never raw filesystem; the
  vault (or any Mneme instance) declares what it exports, Aoide decides what
  it admits.
- Obsidian: integration dendrite shipped, wired, `enable = false` by default —
  a toggle, not a dependency of the Mneme integration.
- Homes: `~/Melete` and `~/Mneme` are their own roots; `~/Aoide` holds only
  Aoide's state. Three roots, three backup targets.

Governance
- Rebuilds are User-gated (polkit pipeline pattern from sakaki's agent-sudo
  design); the agent proposes, User admits, git remembers.
- Policy, lint, and a single audit log live in aoided core; CLI and MCP doors
  inherit the same gate and write the same `~/Aoide/log`.

## 3. Self-ricing — the main component

Aoide ships the rice engine as a builtin, plus 2–3 default rices as pure nix
rice modules under rime/. Everything else it learns.

Commands: `aoide rice gen <prompt|wallpaper>` · `rice lint` · `rice preview`
· `rice adopt <name>` · `rice transpose <rice> <palette>` · `rice list` ·
`rice rm <name>`.

```
┌─ generate ───────────────────────────────┐
│ agent: aoide rice gen <prompt|wallpaper> │
│ reads songbook/ first, always            │
└───────────────┬──────────────────────────┘
                ▼ tokens, token schema
┌─ lint ────────┴──────┐ fail → reject + songbook note
└───────────────┬──────┘
                ▼
┌─ preview · ephemeral ────────────────────┐
│ ~/Aoide/stage/tokens.json                │
│ qs hot-reload · hyprctl · terminal OSC   │
└───────────────┬──────────────────────────┘
                ▼ User: aoide rice adopt
┌─ durable · pure nix ─────────────────────┐
│ Song/repertoire/<song>/ — committed      │
│ rice.nix: tokens import + config swaps   │
└───────────────┬──────────────────────────┘
                ▼ commit · gated rebuild
        every class themed at once
```

`Song/` carries the performed half whole (tree in §2): versioned —
repertoire/ songbook/ keys/ covers/ chimes/ — plus gitignored runtime:
stage/ backstage/ auditions/ catalog/ index/ log/. Per-song contract:

```
Song/repertoire/<song>/
├── rice.nix     pure nix: tokens + swaps
├── tokens.json  token values
├── liner/       design wiki: intent, palette
│                rationale, iteration log
└── assets/      song-specific art, references
```

- `Song/` is the agent's writable domain — it commits there and to nothing
  else; adopt is a commit plus the gated rebuild, nothing more.
- Per-song design knowledge lives in that song's liner/; `songbook/` stays
  cross-cutting: `learnings.md`, `preferences.md` (accumulated from
  adopt/reject decisions), `update-playbook.md` (schema migrations).
- Default seeding: unless told otherwise, `rice gen` starts from the shipped
  default rice + wallpaper — the baseline is always defined, never guessed.
- Songbook discipline: the agent reads songbook/ and the relevant liner/
  before every gen and appends after every adopt or reject — the write-back
  is the "self" in self-ricing; without it this is just a theme generator.
- Dogfood: `Song/repertoire/*/liner/` are ordinary content-pipeline folders,
  pre-approved as system-owned — Aoide ingests its own liner notes, so the
  agent can query past rice reasoning like any other domain.
- Boundary: all of this is Aoide's, per-host, and never stored in the Magi
  vault; Aoide keeps its own knowledge, the vault keeps User's.
- `keys/` is the shared palette library: a key is rice-independent — many
  songs reference one by name, wallpaper extraction emits new ones as
  standalone artifacts, and `rice transpose` replays any song in another
  key, possible only because semantic and component tiers never touch raw
  values.
- Shipped defaults are immutable — evolution lands as new folders under
  `Song/repertoire/`, so a bad generation can never brick the shipped look.

The song map — literal inside Song/ (repo root keeps snowflake names, code
functional verbs): palette = key (keys/) · semantic tier = melody, survives
transposition · component tier = arrangement · facets = instruments · rice =
song (repertoire/) · design wiki = liner/ · memory = songbook/ · wallpaper =
cover (covers/) · sounds = chimes/ · live state = stage/ · plumbing =
backstage/ · propose gate = auditions/ · shipped default = the standard ·
preview = rehearsal · adopt = recording.

Arrangement — what a rice covers (a full rice orchestrates all of it):

```
key          palette
voice        typography — fonts · scale · nerd glyphs
geometry     gaps · radius · borders · blur · animations
instruments  quickshell — the whole shell surface: bar ·
             notifs · launcher · osd · lockscreen · greeter ·
             wallpaper layer · widgets (+ plays chimes)
             compositor (hyprctl, live) · stylix — terminal
             + prompt · gtk/qt + icons + cursor · editors ·
             browser · boot
chimes       notification + system sounds
cover        wallpaper
```

- Everything app-side is a Stylix target — rice.nix feeds the scheme into
  stylix once and it fans out to every nix-manageable app; custom facets
  survive only where stylix doesn't reach or live-reload matters.
- Everything shell-side is a Quickshell widget — no hyprlock, swww, rofi, or
  swaync; one runtime reading stage/tokens.json means nearly the full
  arrangement hot-reloads at rehearsal.
- Coverage tiers: solo (key + cover) · ensemble (core surfaces, the gen
  default) · full orchestration (`rice gen --full`, everything incl.
  greeter, per-app, chimes); `rice lint` reports instrumentation coverage,
  so a full rice missing its lockscreen fails loudly.

Self-update
- Rice self-update: token-schema bumps trigger the update playbook — the
  agent migrates repertoire/ modules, lints, previews, and queues adopts;
  nothing applies without the gate.
- System self-update: `aoide update` (upstream merge) and flake.lock bumps
  (nixpkgs, stylix) are proposed by the agent, applied only through the
  gated rebuild. No background self-updaters anywhere — house policy, same
  as Melete.

## 4. Design-token seam

- Tokens are a standalone package: resolver, schema lint, live-side
  emitters (QML/stage · hyprctl · OSC), exposed to nix as the
  `aoide.tokens` option; every facet consumes tokens and nothing else — no
  module reads another module.
- Stylix is the baked fan-out: rice.nix feeds the same scheme into stylix
  (base16Scheme, fonts, cursor, wallpaper) and stylix themes every
  nix-manageable target; the token package keeps only the live side.
- One source, two fan-outs, zero drift: stage/tokens.json (rehearsal) and
  stylix (recording) both derive from the same tokens — preview and adopted
  state cannot diverge.
- Palette tier = base16, settled — stylix consumes it natively; the open
  schema question shrinks to the semantic and component tiers.
- Wrap prior art before writing a resolver: Style Dictionary and the W3C
  design-tokens format already do tiered refs + multi-format emit; the only
  genuinely missing pieces are the emitters.
- The schema names are owned by the design-system workstream (separate
  chat); until v1 lands, the engine builds against provisional v0 (§8) —
  same shape it will migrate from: palette (base16), typography
  (family/scale), spacing, radius, iconography, wallpaper ref, per-app
  overrides.
- Preview coverage is uneven by nature: quickshell + hyprctl + terminal OSC
  reload live; GTK/Qt mostly need app restarts — preview is the sketch,
  adopt is the truth.

## 5. Onboarding — first boot flow

Target: a fresh box (yomi-strix first) reaches a riced, agent-ready desktop
from one forked repo.

1. Fork upstream, `git clone` your fork to `~/Aoide` — shared history, so
   updates merge cleanly and improvements can flow back.
2. First rebuild from the fork brings up Hyprland + Quickshell + aoided +
   shellbridge with the default rice.
3. `aoide onboard` (idempotent):
   - generates hosts/<hostname>/ from the template and creates Song/'s
     runtime dirs (stage/ backstage/ auditions/ catalog/ index/ log/ —
     gitignored), installs the shipped default rice + wallpaper as the
     active baseline, links `~/Song` → `~/Aoide/Song`, sets the upstream
     remote for `aoide update`,
   - seeds songbook/ with starter files and the update playbook,
   - writes AGENTS.md to the well-known path and prints the tier guide,
   - detects claude CLI; offers the stdio MCP registration line and the spawn
     wrapper install; other agents get the shell instructions,
   - offers integration toggles: mneme source registration (URL + auth),
     obsidian (off unless flipped),
   - walks one approve-gate demo: register a scratch folder, watch it flow
     discover → propose → approve → query,
   - runs `aoide rice preview` on a shipped default to verify the live loop.
4. Done-state check: bar shows agent session + connection state; a
   notification round-trips agent → center; `aoide schema --json` validates.

## 6. The yomi-strix session — first agenda

What this handoff exists for. On-box, User + Claude:
1. Read this doc; confirm or amend the decision log (amendments get one line
   each, appended, dated).
2. Spike a minimal Quickshell NotificationServer (actions + inline reply) —
   commit or fall back from evidence, per §8.
3. Land the token schema v1 from the design-system workstream; wire the
   validator into `rice lint`.
4. Co-design the first real rice: gen → preview loop live on yomi-strix
   hardware, iterate tokens with User, adopt as the box's rice.
5. Seed songbook/preferences.md and the first song's liner/ from that
   session's decisions — the first real write-back.
6. Verify claude CLI integration end to end: wrapper spawn, hook states in
   the bar widget, click-to-jump.

## 7. Build order (v1 slice)

1. `aoide` CLI skeleton + `guide` + `schema --json` — the trunk exists first.
2. shellbridge + Quickshell bar + notif daemon — the body is visible.
3. Rice engine + token schema stub + 2 default rices + preview loop — the
   headline works.
4. Fork template + `aoide onboard` + `aoide update` — fork-and-run is real.
5. Session tracking + claude wrapper + hooks — the agent is present.
6. MCP façade (stdio) generated from schema — the optional door.
7. Content pipeline (discover→…→query) + mneme/obsidian integrations — the
   library opens last.

Note: the management tools threaded through the slices (rice ops · pkg ·
update · onboard · divergence lint) are first-class deliverables — their
command specs are development work, deliberately not pinned here.

## 8. Flags — resolved in design review (2026-07-25)

- Merge hygiene → merge-base divergence lint inside `aoide update` plus a
  commit-hook warning on edits to inherited files; provenance is the
  mechanism, path guards are dead.
- Quickshell NotificationServer → spike first on yomi-strix (§6.2); if
  actions + inline reply land, it keeps the daemon role — no interim
  mako/swaync, the zoo stays dead.
- Claude "blocked" detection → hook state machine (Notification / Stop /
  Pre-PostToolUse) primary; PTY-quiet timer in the wrapper as the universal
  backstop for any agent.
- Melete home-path knob → removed; vestigial since homes reverted to their
  own roots.
- Token schema → provisional v0 now (bg/fg/accent/urgent +
  bar.*/notif.*/window.*) inside the W3C design-tokens container;
  design-system v1 supersedes and the playbook migrates — building the
  engine against no schema was fiction.
- GTK/Qt live preview → accepted as adopt-only; `rice preview --gallery`
  (sample apps restarted under a preview env) is a later nicety, not v1.
- Stylix ↔ Quickshell overlap → the QS facet declares the surfaces it owns,
  stylix disables derive from that declaration, `checks/` asserts no
  surface has two owners.
- Runtime ephemera → `checks/` fails the eval if any module reads Song/
  runtime paths at build time — stage/ can never become load-bearing.

Still open (development work, not design):
- CONTRACTS.md format and versioning granularity.
- Tool-suite command specs (`pkg`, `update`, divergence-lint internals).

## 9. References

- den — github.com/denful/den · pattern prior art only (aspects, quirks) —
  NOT a dependency; dropped for the in-house walker after the pure-nix turn.
- Stylix — base16-driven whole-system theming; the baked fan-out engine.
- Style Dictionary · W3C design-tokens format — token resolver prior art;
  wrap, don't rewrite.
- dxflake — github.com/dxcently/dxflake (dendritic auto-discovery prior art;
  adoption target for the nucleus + dendrite imports).
- herdr agent-guide pattern — herdr.dev/agent-guide.md (prior art for
  tier-0 onboarding docs; herdr itself is NOT a dependency).
- Quickshell, Hyprland IPC, org.freedesktop.Notifications spec, unixporn
  canon for shell patterns.
