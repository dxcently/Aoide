//! strillix-rice — Aoide's ricing/design engine.
//!
//! `theme` is the native design-token engine: schema validation, `{group.key}`
//! deref + component fallback, and the stage/hyprctl/osc/file/kitty/hyprlock emitters.
//! `live` computes the Hyprland
//! geometry/border keyword list a staged notes document implies and
//! (best-effort) applies it to a running compositor, and renders + pushes
//! the staged terminal colours to open kitty windows. `notes`/`live` are
//! dependency-free leaves (Phase 5a). `compose` (the pure `rice compose`
//! scaffolding/rendering engine) and `wallpaper` (wallpaper-art derivation +
//! resolution) land in Phase 5b with a `aoide-storage` dependency
//! (`crate::paths::strillix_root`).

pub mod checkout_gate;
pub mod commands;
pub mod compose;
pub mod wallpaper;
pub mod elements;
pub mod health;
pub mod history;
pub mod host_declare;
pub mod ipc;
pub mod live;
pub mod lock;
pub mod migrate;
pub mod mode;
pub mod paths;
pub mod theme;
pub mod lint;
pub mod wallpaper_provider;
pub mod widgets;

// Every env-touching test in this crate now shares ONE lock —
// `aoide_test_support::env_lock()` (a dev-dependency) — instead of this
// crate-local mutex. Two locks meant `wallpaper.rs`/`live.rs` tests weren't
// serialised against `commands/rice.rs`/`commands/mode.rs`/`commands/draft.rs`
// tests that touch the SAME process-global env vars (`AOIDE_STAGE_DIR`,
// `HYPRLAND_INSTANCE_SIGNATURE`), racing them under the default multithreaded
// test harness. Removed now that nothing in the crate references it.
