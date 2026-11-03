//! The per-user runtime root.

use std::path::PathBuf;

/// The runtime ROOT: `$XDG_RUNTIME_DIR` on either host, else this host's own
/// per-user answer (`/run/user/<euid>` on Unix, `%LOCALAPPDATA%` — falling
/// back to the temp dir when that variable is somehow absent — on native
/// Windows). Each owner appends its own segment.
pub fn runtime_root() -> PathBuf {
    if let Some(dir) = std::env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
    }
    host_default()
}

#[cfg(unix)]
pub fn host_default() -> PathBuf {
    PathBuf::from(format!("/run/user/{}", euid()))
}

#[cfg(windows)]
pub fn host_default() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// This process's effective uid. `std` has no `geteuid(2)` wrapper and the
/// call takes no arguments and cannot fail, so this is the whole of it.
#[cfg(unix)]
pub fn euid() -> u32 {
    // SAFETY: `geteuid(2)` takes no arguments, cannot fail, and touches no
    // memory this process does not already own.
    unsafe { libc::geteuid() }
}
