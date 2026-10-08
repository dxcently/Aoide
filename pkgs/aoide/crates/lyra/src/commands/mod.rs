//! Command groups — lyra's own bundle assembly (P-A4 of the binary-split
//! workstream, docs/architecture/PACKAGE-LAYOUT.md). Each DOMAIN crate
//! contributes its entries via its own `commands::register(&mut Registry)`
//! function — the SAME functions `aoide-cli` calls for these groups; lyra is
//! a second composition root over shared handler code, not a fork of it
//! (Cordis correspondence, plan's "Verified facts" section: an app crate's
//! explicit `register()` list is the bundle's profile, not an import-list
//! violation).
//!
//! [`all`] assembles the graphical bundle in the SAME relative order these
//! groups hold in core's `commands/mod.rs::all()` today: meta (guide/
//! schema/mcp serve — root-coupled, see `commands/infra.rs`'s doc comment
//! for why `mcp serve` is here despite not being named in the plan's group
//! list), onboard (P-I3: the nix half of the onboarding flow — root-coupled
//! like `meta`, appended right after it: a root-level command, registered
//! before `mcp.serve`, matching core's own relative placement of `onboard`
//! in `crates/cli/src/commands/mod.rs::all()`), rice, draft,
//! mode, cover, livery, rice-late stubs (declare/transpose only — NOT
//! content/make/update, which stay core), shellbridge, quickshell
//! (healthcheck only — the placeholder-screen watchdog), reload (the one
//! mode-aware iteration command, `lyra reload` design settled 2026-08-31 —
//! absorbed `quickshell reload` outright, registered right after
//! `quickshell` where that path used to sit), screen, herald, take, element
//! (L-E1, docs/architecture/ELEMENTS.md: `element seed`, the render
//! pipeline's shell-reachable bridge), secrets (P3: `secrets ask`, the
//! rice-shaped TOTP code-entry popup), pair (`pair ask` + `pair show`, the
//! pairing-ceremony's own two dialog shapes — a typed-code entry surface on
//! either direction and a reply-code display surface, sharing `secrets
//! ask`'s six-box QML component's PARENT module (`dialog_qml`) without
//! sharing its entry surface — root-coupled like `meta`/`onboard` in shape),
//! preview (P1: `preview` + `preview.set`, an ISOLATED quickshell canvas for
//! iterating on one widget — its own root under `$XDG_RUNTIME_DIR/aoide-
//! preview`, never the live daemon's socket dir — plus a same-lane
//! follow-up's `preview.declare`, the canvas's own `rice declare`
//! counterpart), preview_tools (P6: `preview.shot` + `preview.tree` +
//! `preview.notes`, shell-first agent tools over that same isolated
//! canvas — a screenshot of the screen/canvas/widget/one element, the
//! canvas's live item tree joined against a static parse of the widget's
//! own QML source, and a small scaffolding-notes store — all appended
//! LAST per golden discipline's "append, never reorder,"
//! `pkgs/aoide/crates/AGENTS.md`). Core-only groups (graph, adapter melete,
//! conductor, a2a serve, agents, nodes, usage, hooks, daemon, soundcheck)
//! are absent — lyra never registers them.
//!
//! `icon` (`icon collections`/`icon list`/`icon resolve`, the pinned Iconify
//! glyphs) and `apps` (`apps list`/`apps show`/`apps publish`, over
//! `crate::xdg`: the installed desktop apps with their resolved icons, and
//! `song/stage/apps.json`) close the list. `registry.rs`'s golden test is the
//! sole authority for the command set (`pkgs/aoide/crates/AGENTS.md`, "Golden
//! discipline").
pub mod apps;
pub mod dialog_qml;
pub mod icon;
pub mod infra;
pub mod meta;
pub mod onboard;
pub mod pair;
pub mod preview;
pub mod preview_tools;
pub mod secrets;
pub mod stubs;

use crate::registry::{Layout, Registry};

/// How `lyra` lists its commands (see core's `LAYOUT` for the contract).
const LAYOUT: Layout = Layout {
    tagline: "paint AoideOS: songs, liveries and the desktop surfaces",
    sections: &[
        ("Start here", &[("guide", ""), ("onboard", "")]),
        (
            "Songs & liveries",
            &[
                ("rice", "Compose, stage, declare and take snapshots of a song"),
                ("livery", "Resolve, lint and emit a livery"),
                ("cover", "Set and sync the live wallpaper"),
                ("reload", ""),
                ("element", ""),
            ],
        ),
        (
            "Widgets & icons",
            &[
                ("preview", "An isolated canvas for one widget: set, shoot, inspect, declare"),
                ("icon", "The pinned Iconify collections, resolved offline"),
            ],
        ),
        ("Desktop apps", &[("apps", "Installed apps and their icons, as the shell lists them")]),
        ("Screen control", &[("screen", "See and drive the desktop: info, shots, pointer, OCR, diff")]),
        (
            "Dialogs & notices",
            &[
                ("secrets", ""),
                ("pair", "Quickshell dialogs for a pairing request and its reply code"),
                ("herald", ""),
            ],
        ),
        ("Agent interfaces", &[("mcp", ""), ("schema", "")]),
        ("System", &[("shellbridge", ""), ("quickshell", "")]),
    ],
};

pub fn all() -> Registry {
    let mut r = Registry::new();

    meta::register(&mut r); // guide, schema
    onboard::register(&mut r); // onboard (P-I3: the nix half of the onboarding flow, root-coupled like meta)
    infra::register_mcp(&mut r); // mcp serve (root-coupled: reads this assembled registry)
    aoide_song::commands::rice::register(&mut r); // rice lint, list, stage, compose
    aoide_song::commands::refresh::register(&mut r); // rice refresh — a built-in song takes the repo's changes to files the machine never edited
    aoide_song::commands::draft::register(&mut r); // rice draft save/list/drop
    aoide_song::commands::mode::register(&mut r); // rice mode status/stage/declarative/draft
    aoide_song::commands::cover::register(&mut r); // cover set
    aoide_song::commands::livery::register(&mut r); // livery emit, resolve, lint
    stubs::register_rice_late(&mut r); // rice declare, transpose
    aoide_conduct::commands::shellbridge::register(&mut r); // shellbridge (own module since P-A2)
    aoide_song::commands::quickshell::register(&mut r); // quickshell healthcheck — placeholder-screen watchdog
    aoide_song::commands::reload::register(&mut r); // reload — the one mode-aware iteration command (absorbed quickshell reload)
    aoide_screen::commands::register(&mut r); // screen info, shot, point *, ocr, diff, send (own crate since P-A1)
    aoide_conduct::commands::herald::register(&mut r); // herald push — dunst's script hook into the notification ledger
    aoide_song::commands::take::register(&mut r); // rice take/take.*, rice back — explicit take-store snapshot
    aoide_song::commands::elements::register(&mut r); // element seed — full render into run/elements/ (L-E1)
    secrets::register(&mut r); // secrets ask — the rice-shaped TOTP code-entry popup (P3)
    pair::register(&mut r); // pair ask + pair show — the pairing-ceremony's own two dialog shapes
    preview::register(&mut r); // preview + preview.set + preview.declare — an isolated quickshell canvas for one widget (P1, then a same-lane follow-up added declare)
    preview_tools::register(&mut r); // preview.shot + preview.tree + preview.notes — shell-first agent tools over that same canvas (P6): screenshots, the live/static-joined item tree, and scaffolding notes
    icon::register(&mut r); // icon.collections + icon.list + icon.resolve — the pinned icon collections (Iconify data) resolved into the lane's own SVG tree, no network at render (I1)
    apps::register(&mut r); // apps list/show/publish — installed desktop apps with resolved icons, song/stage/apps.json

    r.arrange(LAYOUT);
    r
}
