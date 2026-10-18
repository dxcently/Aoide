//! The `rice`/`wallpaper` command groups — the rice domain's CLI commands.
//!
//! Moved from the root package's `src/commands/{rice,wallpaper}.rs` (Phase 9
//! restructure, docs/architecture/PACKAGE-LAYOUT.md): a domain's CLI commands
//! live with the domain. The root package's `commands::all()` calls each
//! `register()` at the exact historical position so `schema --json` order
//! never shifts (rice → draft → mode → wallpaper → theme, keeping the whole
//! `rice` family contiguous).

pub mod wallpaper;
pub mod draft;
pub mod elements;
pub mod theme;
pub mod mode;
pub mod quickshell;
pub mod refresh;
pub mod reload;
pub mod rice;
pub mod history;

#[cfg(test)]
pub(crate) mod test_support;
