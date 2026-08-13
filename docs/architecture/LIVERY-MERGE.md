# Livery — merging the drachma engine into aoide, native + generalized

> **Working title superseded.** Drafted as `THEME-ENGINE-MERGE.md`; renamed to
> `LIVERY-MERGE.md` for the target name this plan recommends (§2). Grep for
> `drachma` still finds every affected file — the rename surface is enumerated
> in §2.2.
>
> **Status: ALL PHASES LANDED — Phase 4 committed and switched
> (2026-08-13).**
> The name is confirmed: **livery** (§7.1). Phase 1 (native engine +
> registry + goldens) reviewed and coach-fixed; Phase 2 (`pkgs/drachma`
> deleted, Node out of the core, vm-boot green); Phase 3 (rename with the
> §2.3 dual-write/dual-read compat) reviewed APPROVE. **Phase 4 (drop the
> mirror/fallback/alias + retarget staged reads, `DrachmaState.qml` →
> `LiveryState.qml`, stale-comment sweep) landed, committed, and switched
> 2026-08-13**; Phase 5 (subdir
> flake) LANDED (2026-08-13): `pkgs/aoide/flake.nix` (nixpkgs-only), root
> consumes the core as the `aoide` path input (topology (b)); toplevel +
> `nix flake check` green.

---

## 0. TL;DR

1. **Port** drachma's resolve/validate/emit pipeline from the ~616-line Node
   CLI (`pkgs/drachma`) into **native Rust inside `crates/song`** — no new
   crate (song's charter already *is* the ricing/design engine). Style
   Dictionary is replaceable by a ~30-line native alias resolver; the Node
   dependency disappears from the core.
2. **Generalize** the emitters behind one `Emitter` trait + registry:
   `stage` · `hyprctl` · `osc` today, `file` (arbitrary config template) added
   as the generalization proof, `gtk`/`gsettings` registered-but-deferred (host
   side-effects belong with the guarded live-apply seam, not the pure engine).
3. **Rename** `drachma → livery` across code, the `aoide.drachma` option
   namespace, songbook data files, the live `stage/drachma.json` contract (with
   a dual-read/dual-write compat window so the running desktop never tears),
   and all docs/wiki.
4. **Sequence:** the merge lands **first**; the separate "subdir flake" work
   item (topology (b), AOIDE-DEV.md §7) lands **after** — the merge makes
   `pkgs/aoide` self-contained Rust, which is a precondition for cleanly
   extracting it into its own flake.

Everything is behavior-preserving for the live system except the deliberate,
compat-gated rename of the runtime stage file.

---

## 1. Target architecture

### 1.1 Where the function lives — extend `crates/song`, no new crate

`crates/song`'s charter (PACKAGE-LAYOUT.md per-crate table): *"The ricing /
design engine: locate `drachma`, apply songs, mint palettes, write the stage,
the Pantheon design language… **Rices portably**."* The drachma engine **is**
that charter — today it is merely *delegated out* to a Node sibling. Folding it
in is completing the crate, not growing a new one.

Decision: **add the engine as a module tree inside `crates/song`**, not a new
`crates/livery`. Rationale (YAGNI, "the design that deletes more"):

- The engine is ~500 lines of pure Rust — squarely song's concern, not an
  independent boundary. A new crate buys a `Cargo.toml`, a workspace member, a
  `pkg-`/dependency edge, and a second "where does theming live?" answer, for
  no reuse (nothing but song consumes it).
- PACKAGE-LAYOUT's own naming verdict: plain/aoide-vocabulary names, voice only
  where the crate is aoide's own; it explicitly *rejected* proliferating small
  crates. song already owns `notes`/`live`/`mint`/`cover`; `livery` joins them.
- The one boundary that DOES matter — pure computation vs. host side-effects —
  already exists inside song (`live::geometry_keywords` pure vs
  `live::apply_live` effectful, kept split "so a future `management` crate could
  lift just the host-effect half out later"). The new engine honors the same
  split (§1.3).

Target module tree (names internal; the public token is `livery`, §2):

```
crates/song/src/
  lib.rs                     # add `pub mod livery;`
  livery/
    mod.rs                   # Resolved, EmitOutput, Emitter trait, registry(), lint(), resolve()
    schema.rs                # port of schema.js — the authoritative v0 validator
    resolve.rs               # native {group.key} deref + component null→palette fallback
    emit/
      mod.rs                 # Emitter trait + `registry() -> &[&dyn Emitter]`
      stage.rs               # → stage JSON (Quickshell)          [ports emitStage]
      hyprctl.rs             # → hyprctl keyword lines            [ports emitHyprctl]
      osc.rs                 # → terminal OSC sequences           [ports emitOsc]
      file.rs                # → arbitrary config-file template   [NEW: generalization proof]
  notes.rs                   # SHRINKS: drop the drachma-binary locate/shell-out;
                             # `run_lint` becomes a thin call into livery::lint
  live.rs                    # unchanged (already the effectful hyprctl seam)
  commands/rice.rs           # `rice lint` → livery::lint (native); preview uses livery::emit
  commands/livery.rs         # NEW: `aoide livery emit|resolve|lint` verb group
```

### 1.2 The emitter registry (additive backend design)

One trait, one registry, one worked example of adding a backend. This is the
"apply a theme to ANY settings system" requirement made structural.

```rust
// crates/song/src/livery/emit/mod.rs
pub struct Resolved {                 // the flat, fully-resolved note set (resolve.rs output)
    pub schema_version: String,
    pub palette: Map<String, String>, // concrete hex, fallbacks applied
    pub base16:  Option<Map<String, String>>,
    pub bar:     Map<String, String>,
    pub notif:   Map<String, String>,
    pub window:  Map<String, String>,
    pub geometry: Option<Value>,       // passthrough (never validated by the engine, CONTRACTS §1)
}

pub enum EmitOutput {
    Json(serde_json::Value),  // stage
    Lines(Vec<String>),       // hyprctl argv lines
    Text(String),             // osc stream, file-template render
}

pub struct EmitOpts<'a> { pub template: Option<&'a str>, /* file backend etc. */ }

pub trait Emitter: Sync {
    fn target(&self) -> &'static str;                 // "stage" | "hyprctl" | "osc" | "file"
    fn emit(&self, r: &Resolved, o: &EmitOpts) -> Result<EmitOutput, EmitError>;
}

pub fn registry() -> &'static [&'static dyn Emitter] {
    &[&stage::Stage, &hyprctl::Hyprctl, &osc::Osc, &file::FileTemplate]
}
```

Adding a backend = **one new `emit/<name>.rs` + one line in `registry()`** — the
same additive discipline as pkgs walker / dendrites / song widgets. The `file`
backend (substitute `{{palette.bg}}`-style placeholders in a caller-supplied
template) is implemented now as the generalization proof; it is the "arbitrary
config-file emitter" and is pure (no host effect, low risk).

### 1.3 Pure emit vs. host apply — the seam that stays

```
livery::emit::hyprctl  →  Vec<String> keyword lines      (PURE — testable, golden-able)
       live::apply_live(lines)  →  runs `hyprctl --batch` (EFFECT — env-guarded, best-effort, non-fatal)

livery::emit::stage    →  serde_json::Value               (PURE)
       shellbridge::atomic_write(stage/…, …)              (EFFECT — atomic write-temp-rename)

livery::emit::gtk      →  gtk.css / gsettings key list     (PURE)   [DEFERRED backend]
       management/live dconf-load / write ~/.config/…     (EFFECT)  [DEFERRED — host op]
```

`gtk`/`gsettings` **live-apply** (dconf load, writing into the user's live GTK
config) is a *host side-effect* of the same class PACKAGE-LAYOUT reserves for
the (deferred) `management` crate. The PURE emit half may be added to the
registry any time; the EFFECT half is deferred, consistent with the existing
`management`-deferral. This is the explicit YAGNI line: **the engine ships four
pure backends (parity + generalization proof); host-mutating backends are
additive follow-ons, not merge scope.**

### 1.4 Port analysis — what moves, what dies, what cannot port

| pkgs/drachma piece | Native Rust home | Notes |
|---|---|---|
| `schema.js` `validate()` + HEX/PALETTE/BASE16/COMPONENT_FALLBACK | `livery/schema.rs` | Pure predicate over `serde_json::Value`. Direct port. Keep the exact error strings so goldens/messages match. |
| `resolve.js` `dereference()` (Style Dictionary `exportPlatform`) | `livery/resolve.rs` `deref()` | **Style Dictionary is only used to resolve `{group.key}` aliases.** v0 is closed + single-level; a native pass (lookup `{a.b}` → tree[a][b], cycle-guarded) replaces it. Shipped songs use **zero** aliases (verified: `default`/`sonata` are concrete hex); only the test fixture `valid.json` exercises refs. |
| `resolve.js` component null→palette fallback | `livery/resolve.rs` | Already duplicated in Nix facets (stylix/quickshell) — the Rust copy is the third implementation of the same rule; keep them identical. |
| `emitters.js` `emitStage`/`emitHyprctl`/`emitOsc` | `livery/emit/{stage,hyprctl,osc}.rs` | Direct port. OSC/hyprctl are byte-order-sensitive → byte-parity goldens. |
| `cli.js` (arg parsing, atomic write, exit codes) | `commands/livery.rs` + existing `shellbridge::atomic_write` | The CLI shell is replaced by aoide's `Invocation`/`Outcome` + registry. `atomicWriteJson` already exists natively (`aoide_storage::fs::atomic_write`). |
| `test/run.js` + fixtures | `crates/song/tests/goldens/` + `#[cfg(test)]` | Fixtures copied to `crates/song/tests/fixtures/`; assertions become Rust `#[test]`s + byte-parity goldens. |
| `default.nix`, `package.json`, `package-lock.json`, `node_modules` | **deleted** | The Node toolchain leaves the core. |

**Nothing in `pkgs/drachma` fails to port.** The only external dependency
(Style Dictionary) is used for one narrow capability (single-level alias deref)
that v0's closed schema makes trivial to reimplement. Stated explicitly per the
directive: *there is no drachma capability that cannot become native Rust.*

---

## 2. The rename

### 2.1 Proposed name — `livery` (recommended; khoa confirms, §7)

> **`livery`.** The drachma name was minted for a *token* layer — a coin
> standing for a colour value — and it earned it while the thing was only a bag
> of colours. But the merge generalizes it into an engine that dresses *every*
> surface — terminal, compositor, GTK, the Quickshell stage, any config file —
> in the one song's identity, and a coin does not clothe a stage. A **livery**
> is exactly that: the single set of house colours a whole retinue wears in
> unison, so that a servant, a ship, and a herald are read at a glance as one
> household's. It keeps drachma's virtue — one word for the values *and* the act
> of stamping them (a livery is both the colours and the wearing of them, just
> as a currency is inseparable from its mint) — while naming the new reach: the
> venue's every instrument now performs the song's livery. It is plain,
> grep-clean (no hit anywhere in-tree, no collision with the live `mneme`/
> `melete` servers), and reads right at every site: `aoide.livery.palette`, a
> song's `livery.json`, `aoide livery emit hyprctl`.

Greek alternative on record for §7: **`kosmos`** (κόσμος — "adornment + order",
the root of *cosmetic*): more on-Pantheon, but abstract and grander than the
concrete "one look worn across surfaces" the engine actually does. Rejected as
primary on the same grep-ability/plainness grounds PACKAGE-LAYOUT used to reject
the deep-pantheon crate scheme; offered as the in-voice runner-up.

> **This plan is written assuming `livery`. If khoa picks another token,
> substitute it everywhere `livery`/`Livery` appears — the structure is
> name-independent.**

### 2.2 Full rename surface (grep census — every consumer)

**Code — Rust (`pkgs/aoide/crates/`)**
- `song/src/notes.rs` — drop `AOIDE_DRACHMA_BIN`/PATH locate + `run_lint` shell-out (becomes native call).
- `song/src/commands/rice.rs` — `rice lint` summary + delegate; `handle_rice_preview` stage-file constant; test strings/fixtures.
- `song/src/mint.rs` — renders `aoide.drachma.<tier>` → `aoide.livery.<tier>`; scaffolds `drachma.json` → `livery.json`; comment text.
- `song/src/commands/livery.rs` — **new** verb group.
- `storage/src/fs.rs` — `songbook_notes()` → `…/livery.json`; test paths (`fs.rs:105,373,379`).
- `storage/src/design.rs:98` — `sources: vec!["stage/livery.json"]`.
- `conductor/src/app.rs:288,295,362,382` — stage reader path `drachma.json` → `livery.json` (dual-read, §2.3).
- `conductor/src/theme.rs:11` — comment.
- `conduct/src/graph/verbs.rs:256` — comment ("mirrors the `livery emit stage` pattern").
- `cli/src/guide.rs:106` — house-rule #5 text (`aoide.livery`).
- `cli/tests/conductor_integration.rs:57,89,92` + `cli/tests/fixtures/seed.sh` — stage fixture filename + assertion text.
- `cli/src/registry.rs` — golden command-path snapshot (updates when `livery.*` verbs land).

**Code — Nix**
- `modules/nucleus/options.nix` — `options.aoide.drachma` → `options.aoide.livery` (+ optional `mkRenamedOptionModule` alias, §2.3).
- `modules/facets/quickshell/default.nix` — `t = config.aoide.livery`; `activeSongNotes`/`seedStageScript` (`drachma.json`→`livery.json`, stage dual-write); `AOIDE_WALLPAPER`/wallpaper reads (`config.aoide.livery.wallpaper`).
- `modules/facets/stylix/default.nix` — `t = config.aoide.livery`.
- `modules/facets/compositor/default.nix` — `t = config.aoide.livery`; comments referencing the engine.
- `modules/nucleus/packages.nix` — **remove** `pkgs.drachma` (Phase 2).
- `lib/pkgs.nix` — no edit needed (walker auto-drops the deleted dir); `flake.nix` comment `pkg-drachma` (Phase 2).
- `lib/vmTest.nix` — **remove** `machine.succeed("which drachma")`; comment (Phase 2).
- `hosts/yomi-strix/default.nix` — grep hit; verify whether it sets `aoide.drachma` (rename if so) or only references.

**Data / songbook**
- `song/songbook/default/drachma.json` → `livery.json`.
- `song/songbook/sonata/drachma.json` → `livery.json`.
- `song/songbook/default/rice.nix`, `song/songbook/sonata/rice.nix` — `aoide.drachma.*` → `aoide.livery.*`.
- `song/songbook/update-playbook.md` — add migration note (§6).

**Live contract — QML (`modules/facets/quickshell/qml/`)**
- `DrachmaState.qml` — `notePath` (`stage/drachma.json` → `stage/livery.json`, dual-read §2.3). Optional file rename `DrachmaState.qml` → `LiveryState.qml` (updates ~12 QML importers: `shell.qml`, `AoideBar.qml`, `NotificationCard.qml`, `GrimoireLedger.qml`, `MoodFaces.qml`, `WorkspaceRow.qml`, `UsageGadget.qml`, `ConductorPreview.qml`, `TerminalsPreview.qml`, `StagingEngine.qml`, `slots.md`). **Recommendation: rename the singleton file too** (a `DrachmaState` in a livery world is exactly the drift khoa is removing) — but this is the widest-blast edit; see §7 for the "how far does the QML rename go" decision.

**Contracts / docs / wiki**
- `CONTRACTS.md` §1 (note schema — `aoide.drachma`→`aoide.livery`), §4 (`stage/drachma.json`→`stage/livery.json`), §5 (song shape — "a song sets ONLY `aoide.livery`").
- `AGENTS.md` house-rule #5 ("Facets read only `aoide.livery`").
- `README.md` (§`drachma — the note engine` → livery; the pkgs tree line; the facet-read lines; the CLI table).
- `docs/architecture/PACKAGE-LAYOUT.md` (song charter mentions `drachma`), `docs/BUILD.md`, `docs/architecture/aoide-report.html`.
- Wiki: `docs/Aoide-Wiki/entities/drachma.md` → `livery.md` (+ aliases), and ~28 pages that mention drachma (Codebase, Self-Ricing, Song-Anatomy, Song-Vocabulary, Ricing-Protocol, Lexicon, SCHEMA, Full-Architecture, Snowflake-Anatomy, Widget-Bridge-Contract, Stylix, Quickshell, Gadget-Dock, Feature-Set, Widget-Maker, AOIDE-DEV §7, etc.). Delegated to the librarian per AOIDE-DEV §6.

### 2.3 Live-contract compat path — dual-write + fallback-read, then drop

The desktop is **running** on `stage/drachma.json`, read live by three
processes (QML `DrachmaState`, the conductor TUI `app.rs`, and re-seeded by home
activation) and written by two (`rice preview`, the seed script). A bare rename
tears the live desktop the moment a new writer runs against an old reader (or a
new reader starts before any new write). The solid path:

```
                 OLD reader (running QML/conductor)      NEW reader (post-switch)
   Phase 3 write:  reads stage/drachma.json  ◄── legacy mirror ──┐
                                                                  │  writers write BOTH:
   Phase 3 read:   reads stage/livery.json first,                │    stage/livery.json  (canonical)
                   falls back to stage/drachma.json ─────────────┘    stage/drachma.json (legacy mirror)
   Phase 4:        drop the legacy mirror write + the fallback read
```

- **New readers** (`DrachmaState.qml` FileView, `conductor/app.rs`): read
  `stage/livery.json`; if absent, fall back to `stage/drachma.json`. So a fresh
  shell that starts before any new write still finds the file the old seed left.
- **New writers** (`rice preview` in `commands/rice.rs`, the `seedStageScript`
  in the quickshell facet): write `stage/livery.json` **and** mirror to
  `stage/drachma.json`. So an old still-running conductor/QML (not yet
  restarted) keeps rendering after a new `rice preview`.
- **Nix option namespace** needs no runtime compat (everything rebuilds
  atomically), but add `lib.mkRenamedOptionModule [ "aoide" "drachma" ] [ "aoide"
  "livery" ]`-style aliases in `modules/nucleus/options.nix` so an out-of-tree
  host set (or a stale `song/songbook/*/rice.nix` mid-migration) still evaluates
  during the transition. Drop the alias in Phase 4.
- **Phase 4** (gated on khoa confirming the switched desktop is stable): remove
  the legacy-mirror write, the fallback read, and the option alias.

This guarantees: **at every instant of the transition, both an old and a new
reader find a valid, non-torn file.** No regression window.

---

## 3. Sequencing — merge FIRST, subdir-flake SECOND

**Recommendation: land the whole drachma→livery merge (Phases 1–4) before the
separate subdir-flake work item (topology (b), AOIDE-DEV.md §7 "Separate Aoide
from AoideOS").**

Why merge-first (not after, not interleaved):

- The merge makes `pkgs/aoide` **self-contained Rust** — it deletes the
  cross-package runtime dependency where `aoide` shells out to a *separate*
  `drachma` binary (`pkgs.drachma` on the system profile, located via
  `AOIDE_DRACHMA_BIN`/PATH). That cross-package seam is exactly what would
  become a cross-*flake* seam if the subdir flake landed first: a
  `pkgs/aoide/flake.nix` package shelling out to a `drachma` binary the *root*
  flake's walker provides is an awkward interim (the subdir flake can't own
  `pkgs/drachma`, its sibling). Merge-first deletes the seam, so the subdir
  flake inherits one clean single-language package.
- The merge is fully expressible under the **current** topology (walker-based
  `lib/pkgs.nix`): it removes one walked package (`drachma`) and folds its
  function into the already-walked `aoide` package. No flake restructure needed
  to keep it green.
- No interleave is required. The only shared touch-point — dropping `nodejs`
  from `flake.nix`'s `devShells` — is merge cleanup (Phase 2), independent of
  the subdir work.

**Combined step list (top level):**

```
MERGE (this plan)                                          then   SUBDIR FLAKE (separate item)
─────────────────────────────────────────────────────────       ────────────────────────────
Phase 1  native engine in crates/song (still drachma-named        Phase 5  pkgs/aoide/flake.nix;
         on the wire; live system untouched)                               root consumes as path
Phase 2  cut pkgs/drachma from the walker + all pkgs.drachma               input (topology b).
         refs; drop Node toolchain                                         Precondition MET by
Phase 3  rename drachma → livery (options, data files,                     Phases 1–4: pkgs/aoide
         stage-file compat, docs)                                          is now self-contained
Phase 4  drop the stage-file legacy mirror (gated on live-                 Rust — no Node sibling.
         stable confirmation)
```

Each phase leaves `nix build .#nixosConfigurations.yomi-strix…toplevel` +
`cargo test --workspace` green before the next begins.

---

## 4. Ordered, executor-ready step list

Conventions: **[SERIAL]** must follow the prior step; **[PARALLEL-SAFE]**
non-overlapping writer paths, may run concurrently with siblings. Every step
lists the exact files it owns and its acceptance commands. Commit with explicit
pathspecs (`git add -- <listed paths>`), never `git add -A` (shared-worktree
discipline, AOIDE-DEV §5). No AI co-author trailer.

Pre-req gate for all steps: `nix develop` (Rust + nix tools) available;
`nix build .#drachma` builds the current Node binary (needed to capture goldens
in Step 1.1).

---

### PHASE 1 — Native engine (internal only; external `drachma` contract unchanged)

Phase 1 is **one executor, sequential sub-steps** — it is a single crate with
tight module coupling (schema ← resolve ← emit ← commands), not independent
files. Do **not** parallelize across agents (same lesson as PACKAGE-LAYOUT
Phase 6). The live system is untouched: still writes `stage/drachma.json`, still
reads `aoide.drachma`-shaped notes; `pkgs/drachma` remains present but aoide
stops calling it.

**Step 1.0 — capture goldens from the current Node engine.** [SERIAL, first]
- Owns (new): `pkgs/aoide/crates/song/tests/fixtures/*.json` (copied from
  `pkgs/drachma/test/fixtures/`), `pkgs/aoide/crates/song/tests/goldens/`.
- Do: `nix build .#drachma -o /tmp/drachma`; for each fixture × target, save
  `/tmp/drachma/bin/drachma resolve <fix>` and `… emit {stage,hyprctl,osc} <fix>`
  to `tests/goldens/<fixture>.<target>.golden`.
- Accept: goldens committed; `ls tests/goldens` shows resolve+3 targets × 3
  fixtures (valid, valid-hot, valid-base16).

**Step 1.1 — `livery/schema.rs` (validator).** [SERIAL]
- Owns: `crates/song/src/livery/mod.rs` (skeleton + `pub mod schema;`),
  `crates/song/src/livery/schema.rs`, `crates/song/src/lib.rs` (add `pub mod
  livery;`).
- Do: port `schema.js::validate` + `HEX`/`PALETTE_KEYS`/`PALETTE_OPTIONAL_KEYS`/
  `BASE16_KEYS`/`COMPONENT_FALLBACK`; preserve exact error strings.
- Accept: `cargo test -p aoide-song schema` — unit tests mirroring
  `pkgs/drachma/test/run.js` cases 1–9 (hot accept/reject, base16
  accept/missing/hex/unknown, closed-palette) all pass.

**Step 1.2 — `livery/resolve.rs` (native deref + fallback).** [SERIAL]
- Owns: `crates/song/src/livery/resolve.rs`, `livery/mod.rs` (`Resolved` struct).
- Do: single-level `{group.key}` alias resolver (cycle-guarded), then component
  null/empty→palette fallback per `COMPONENT_FALLBACK`; `withHash`/`stripHash`
  helpers. Output the flat `Resolved`.
- Accept: `cargo test -p aoide-song resolve`; golden parity:
  `diff <(cargo run -p aoide-cli -- livery resolve tests/fixtures/valid.json)
  tests/goldens/valid.resolve.golden` — **byte-identical** (verifies the native
  deref matches Style Dictionary on the ref-using fixture).

**Step 1.3 — `livery/emit/` (registry + stage/hyprctl/osc).** [SERIAL]
- Owns: `crates/song/src/livery/emit/{mod,stage,hyprctl,osc}.rs`.
- Do: `Emitter` trait + `registry()`; port `emitStage`/`emitHyprctl`/`emitOsc`.
  Stage output shape must match CONTRACTS §4 exactly (schemaVersion, palette,
  bar, notif, window, optional base16).
- Accept: `cargo test -p aoide-song emit`; byte-parity goldens for `emit osc`
  and `emit hyprctl` (order-sensitive → byte-identical); for `emit stage` assert
  **semantic JSON equality** (parse + compare — serde's key order legitimately
  differs from `JSON.stringify`; QML reads by key, so shape-equality is the
  contract, byte-identity is not).

**Step 1.4 — file-template backend (generalization proof).** [SERIAL]
- Owns: `crates/song/src/livery/emit/file.rs`, `emit/mod.rs` (register it).
- Do: pure template renderer — substitute `{{palette.bg}}`/`{{window.border}}`
  etc. placeholders in a caller-supplied template string; unknown placeholder →
  structured error.
- Accept: `cargo test -p aoide-song emit::file` — a template round-trips a
  known set of substitutions; an unknown placeholder errors, not panics.

**Step 1.5 — wire CLI: `rice lint` native + `livery` verb group.** [SERIAL]
- Owns: `crates/song/src/notes.rs` (shrink to a thin `livery::lint` wrapper;
  drop `AOIDE_DRACHMA_BIN`/PATH locate — but keep a `run_lint` shape its two
  callers expect), `crates/song/src/commands/rice.rs` (`handle_rice_lint` →
  native; `rice lint` summary drop "drachma"; `handle_rice_preview` uses
  `livery::emit::hyprctl` for the keyword batch instead of the inline
  `live::geometry_keywords`? — **no**, keep `live::geometry_keywords` as-is;
  only `rice lint` changes engine), `crates/song/src/commands/livery.rs` (new:
  `livery emit <target> [<song>|<path>]`, `livery resolve …`, `livery lint …`),
  `crates/song/src/commands/mod.rs` (register the group).
- Do NOT touch: `stage/drachma.json` filename (stays until Phase 3), any
  `aoide.drachma` Nix, any songbook file.
- Accept: `cargo test --workspace` green; `cargo run -p aoide-cli -- rice lint
  song/songbook/sonata/drachma.json` validates natively (no `AOIDE_DRACHMA_BIN`,
  no PATH `drachma`); `aoide schema --json` diff is **reviewed** and contains
  ONLY: the three new `livery.*` command entries + the `rice.lint` summary edit
  (`registry.rs` golden snapshot updated to match, in this step).

**Phase 1 exit gate:** `cargo test --workspace`; `nix build .#pkg-aoide`;
`nix build .#nixosConfigurations.yomi-strix.config.system.build.toplevel
--no-link`; `aoide rice preview sonata` still stages `stage/drachma.json`
byte-for-byte as before (live desktop hot-reloads identically). `pkgs/drachma`
untouched and still builds (`nix build .#drachma`) — it is now dead weight,
removed next phase.

---

### PHASE 2 — Cut the Node package

**Step 2.1 — delete `pkgs/drachma` + remove every `pkgs.drachma` reference.**
[SERIAL after Phase 1]
- Owns: delete `pkgs/drachma/` (git rm the whole dir);
  `modules/nucleus/packages.nix` (drop `pkgs.drachma` from `systemPackages`);
  `lib/vmTest.nix` (drop `machine.succeed("which drachma")` + its comment);
  `flake.nix` (fix the `pkg-drachma` mention in the checks comment);
  `flake.nix` `devShells` (drop `nodejs` — no other Node consumer remains);
  `crates/song/src/notes.rs` (remove any now-dead `AOIDE_DRACHMA_BIN` residue).
- Accept: `nix flake check` no longer lists `pkg-drachma`;
  `nix build .#nixosConfigurations.yomi-strix…toplevel --no-link` green;
  `nix build .#checks.x86_64-linux.vm-boot` green (KVM host) — boots without a
  `drachma` binary on PATH; `rg -n "pkgs\.drachma|AOIDE_DRACHMA_BIN|which drachma"`
  returns nothing.

**Phase 2 exit gate:** the core ships no `drachma` binary; ricing works
entirely native. `pkgs/aoide` is now self-contained Rust (the subdir-flake
precondition, §3).

---

### PHASE 3 — Rename `drachma → livery`

Phase 3 steps **serialize** (they share the song + storage crates and the
quickshell facet; a half-applied rename breaks eval/build). Only Step 3.4
(docs) is **[PARALLEL-SAFE]** — it owns only docs/wiki, no code.

**Step 3.1 — Nix option namespace + all readers + songbook `rice.nix` +
mint renderer (ATOMIC — keeps `toplevel` eval green).** [SERIAL]
- Owns: `modules/nucleus/options.nix` (`aoide.drachma`→`aoide.livery`; add
  `mkRenamedOptionModule` alias); `modules/facets/quickshell/default.nix`,
  `modules/facets/stylix/default.nix`, `modules/facets/compositor/default.nix`
  (`t = config.aoide.livery`; comments); `song/songbook/default/rice.nix`,
  `song/songbook/sonata/rice.nix` (`aoide.drachma.*`→`aoide.livery.*`);
  `crates/song/src/mint.rs` (renders `aoide.livery.<tier>`; comment text);
  `hosts/yomi-strix/default.nix` (rename iff it sets `aoide.drachma`).
- Accept: `nix build .#nixosConfigurations.yomi-strix…toplevel --no-link` green;
  `nixfmt --check` on every changed `.nix`; `cargo test -p aoide-song mint`
  (mint renders `aoide.livery`); a note file set via the `mkRenamedOptionModule`
  alias still evaluates (transition safety).

**Step 3.2 — songbook data files + the code that resolves them (ATOMIC).**
[SERIAL]
- Owns: `git mv song/songbook/default/drachma.json …/livery.json`;
  `git mv song/songbook/sonata/drachma.json …/livery.json`;
  `crates/storage/src/fs.rs` (`songbook_notes` → `…/livery.json`; test paths);
  `modules/facets/quickshell/default.nix` (`activeSongNotes` + `seedStageScript`
  source path → `livery.json`); `crates/song/src/commands/rice.rs`
  (`handle_rice_mint` scaffolds `livery.json`; `mint` tests + fixtures).
- Accept: `cargo test --workspace` (fs + mint tests green);
  `nix build .#nixosConfigurations.yomi-strix…toplevel --no-link` green (facet
  reads the renamed active-song file); the seed script sources
  `songbook/<song>/livery.json`.

**Step 3.3 — live stage-file rename with dual-read/dual-write compat (ATOMIC).**
[SERIAL] — the desktop-safety step (§2.3).
- Owns: **writers** — `crates/song/src/commands/rice.rs` (`handle_rice_preview`
  writes `stage/livery.json` + mirrors `stage/drachma.json`),
  `modules/facets/quickshell/default.nix` (`seedStageScript` writes both);
  **readers** — `modules/facets/quickshell/qml/DrachmaState.qml` (`notePath`
  → `livery.json`, fallback `drachma.json`), `crates/conductor/src/app.rs`
  (stage read → `livery.json`, fallback `drachma.json`; mtime watches);
  **strings** — `crates/storage/src/design.rs:98`,
  `crates/conductor/src/theme.rs:11`, `crates/conduct/src/graph/verbs.rs:256`,
  `crates/cli/tests/conductor_integration.rs` + `tests/fixtures/seed.sh`.
- Optional (see §7): rename `DrachmaState.qml` → `LiveryState.qml` + its ~12 QML
  importers. **Recommend deferring the QML *file* rename to a follow-up** to
  keep this desktop-critical step's blast radius small; rename only the *path
  constant* here.
- Accept: `cargo test --workspace`; `qs -p
  modules/facets/quickshell/qml/shell.qml` loads clean; `aoide rice preview
  sonata` writes BOTH `stage/livery.json` and `stage/drachma.json`, both
  parse; a conductor launched reading only the legacy file still colours (manual
  smoke); **khoa live vision-check** on the switched desktop (Ricing-Protocol,
  AOIDE-DEV §3) — bar/terminal/gadget still coherent after `rice preview`.

**Step 3.4 — docs / contracts / wiki.** [PARALLEL-SAFE with 3.1–3.3 code, but
land after 3.3 so it describes the shipped state]
- Owns: `CONTRACTS.md` (§1/§4/§5), `AGENTS.md` (#5), `README.md`,
  `docs/architecture/PACKAGE-LAYOUT.md`, `docs/BUILD.md`,
  `docs/architecture/aoide-report.html`, `song/songbook/update-playbook.md`
  (migration note), and the wiki (`entities/drachma.md`→`livery.md` + ~28 pages)
  — the wiki delegated to the librarian (AOIDE-DEV §6).
- Accept: `rg -n "drachma" CONTRACTS.md AGENTS.md README.md docs/` returns only
  intentional historical references (e.g. changelog/migration lines); the
  playbook records the `aoide.drachma`→`aoide.livery` + file rename.

**Phase 3 exit gate:** full workspace tests, toplevel eval, vm-boot, `qs` load,
conductor smoke, and khoa's live vision-check all green. The desktop runs on
`stage/livery.json`; the legacy mirror is still written (dropped in Phase 4).

---

### PHASE 4 — Drop the legacy stage mirror (cleanup)

**Step 4.1 — remove dual-write + fallback-read + option alias.** [SERIAL,
GATED on khoa confirming the switched desktop is stable across a reboot]
- Owns: `crates/song/src/commands/rice.rs` (drop the `stage/drachma.json`
  mirror write), `modules/facets/quickshell/default.nix` (seed writes only
  `livery.json`), `DrachmaState.qml` + `crates/conductor/src/app.rs` (drop the
  `drachma.json` fallback), `modules/nucleus/options.nix` (drop the
  `mkRenamedOptionModule` alias).
- Accept: `cargo test --workspace`; `nix build …toplevel --no-link`;
  `qs -p shell.qml` loads; `rg -n "drachma"` over `pkgs/aoide/`,
  `modules/`, `song/`, `lib/` returns nothing (save deliberate history notes).

---

## 5. Review rubric (applied per step by the pi/deepseek reviewer)

The reviewer coaches via SendMessage, not bare pass/fail (AOIDE-DEV §2.1).
Check, per step:

| Axis | What to verify |
|---|---|
| **Correctness** | Named acceptance commands actually run and pass on the reviewer's checkout — not just claimed. Goldens are byte-identical where the step says byte-identical, semantic-equal where it says semantic. |
| **Contract fidelity** | `aoide schema --json` diff is *exactly* the intended entries for this step and nothing incidental (Phase 1.5/3). `stage/livery.json` shape matches CONTRACTS §4 (schemaVersion + palette + bar/notif/window fully resolved, no `null`). No schema *version* bump (shape is unchanged; the rename is a namespace edit — §6). |
| **Scope discipline** | Only the step's listed files changed (`git diff --stat` matches). No `git add -A`; explicit pathspecs. No file another agent owns was touched. |
| **House rules** | Facets read only `aoide.livery` (no cross-module read); no `song/` runtime path read at eval (`checks.no-song-read` still green); `checks.song-shape` still green (songbook entries are `rice.nix` + `livery.json`, no stray `.nix`); no AI co-author trailer. |
| **Live safety** (Phases 3–4) | The dual-read/dual-write invariant of §2.3 is intact at the step boundary: at no committed state can a running old reader or a fresh new reader fail to find a valid stage file. Phase 4 only lands after explicit live-stable sign-off. |
| **Port faithfulness** (Phase 1) | The native validator's error strings and the emit outputs match the Node engine on the shared fixtures. The alias resolver handles the `{group.key}` fixture and rejects a cycle. |
| **Tests present** | New logic carries `#[test]`s / goldens; a step that ports a Node `test/run.js` case has the Rust equivalent. `cargo test --workspace` is green, not just the crate's own. |

---

## 6. Risks

- **Live desktop runs on the current contract.** Three live readers +
  two writers of `stage/drachma.json` (§2.3). *Mitigation:* the dual-write +
  fallback-read + Phase-4-drop sequence — no instant where a reader finds no
  file. The riskiest single step (3.3) carries a mandatory khoa live
  vision-check before it is called done.
- **NixOS option rename breaks the host config.** `aoide.drachma` is set by
  both songbook `rice.nix` and read by three facets; a half-rename fails
  `toplevel` eval and blocks the gated switch. *Mitigation:* Step 3.1 is atomic
  (option + every reader + every setter in one step) + a `mkRenamedOptionModule`
  alias covers any out-of-tree/stale setter during transition; `nixfmt --check`
  and a full toplevel eval gate the step.
- **Songbook content migration.** Renaming `song/songbook/*/drachma.json` is a
  contract-surface edit; a missed reader (facet `activeSongNotes`, `fs.rs`,
  the seed script) silently breaks the active song's fan-out. *Mitigation:*
  Step 3.2 bundles the file `git mv` with every resolver in one atomic step;
  `checks.song-shape` + the seed activation exercise the path on switch.
- **Byte-drift in the port.** OSC/hyprctl output is order- and byte-sensitive;
  a subtly different emit desyncs terminal/compositor from the baked side.
  *Mitigation:* byte-parity goldens captured from the Node engine *before* it is
  deleted (Step 1.0), asserted in 1.3.
- **Style-Dictionary semantics.** The native deref must match SD on multi-key or
  chained aliases *if any song ever uses them*. Today none do (only the fixture
  does, single-level). *Mitigation:* the resolver is cycle-guarded and
  golden-checked against the ref-using fixture; if a future song needs chained
  aliases, that is an additive resolver enhancement, not a v0 blocker (flag it).
- **QML `DrachmaState` file rename blast radius.** Renaming the singleton file
  touches ~12 importers; doing it inside the desktop-critical Step 3.3 widens
  its risk. *Mitigation:* rename only the *path constant* in 3.3; defer the QML
  *file* rename to a separate, non-critical follow-up (§7).
- **Wiki/CONTRACTS drift owed.** ~30 docs mention drachma; the report HTML and
  CONTRACTS are load-bearing. *Mitigation:* Step 3.4 + librarian brief; the
  playbook migration note is the durable record.
- **vm-boot needs KVM.** The `vm-boot` gate (Phases 2–3) requires a KVM-capable
  build host; on a host without it, substitute the `toplevel` eval + a manual
  `which`-free boot check and flag the gap.

---

## 7. Open decisions for khoa (needed before execution)

1. **The name — BLOCKING.** Recommend **`livery`** (§2.1: names both the token
   values and the act of dressing every surface in them; grep-clean; no
   `mneme`/`melete` collision). In-voice Greek alternative on record: `kosmos`.
   *Execution cannot start until this is confirmed* — the whole plan is
   name-parametric; pick one and it substitutes everywhere.
2. **How far does the QML rename go?** Rename only the stage *path* (`livery.json`)
   and leave the `DrachmaState.qml` singleton file name, **or** also rename the
   file → `LiveryState.qml` (+ ~12 importers)? *Recommendation:* rename the file
   too (a `DrachmaState` in a livery world is the exact drift being removed) —
   but as a **separate follow-up after Phase 3.3**, not inside the
   desktop-critical step. Confirm you want the file rename at all.
3. **Contract version — bump or not?** The rename changes the `aoide.drachma`
   namespace and `stage/drachma.json` filename but **not the schema shape**
   (palette/component/base16/geometry tiers are byte-identical in structure).
   *Recommendation:* keep `v0`, record the rename in
   `song/songbook/update-playbook.md` and update CONTRACTS §1/§4/§5 prose in
   place — no `v0→v1` bump (that's reserved for the semantic/component
   design-system work). Confirm you don't want a formal version bump.
4. **`gtk`/`gsettings` scope.** The plan ships four *pure* backends (stage,
   hyprctl, osc, file-template) and defers the *host-mutating* gtk/gsettings
   **apply** (dconf/live-config write) to the same seam as the deferred
   `management` crate. *Recommendation:* ship the pure `gtk.css`/`gsettings`
   *emit* as an additive backend whenever wanted, keep the live-apply deferred.
   Confirm gtk/gsettings live-apply is out of scope for this merge.
