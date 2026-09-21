//! Sibling-binary resolver: how one aoide process locates the OTHER aoide
//! binary (core `aoide` from lyra's side is not this phase's problem — P-A6
//! only wires conduct's shellbridge, which needs `aoide` for itself and
//! `lyra` for the rice-mode toggle) without hardcoding a bare name that
//! could resolve to nothing off `PATH`.
//!
//! Three tiers, tried in order:
//!   1. an explicit env var override (`AOIDE_CORE_BIN`/`AOIDE_RICE_BIN`) —
//!      the tier a nix unit sets across store-path boundaries, where
//!      sibling resolution cannot work (P-A8: `lyra` moves to its own
//!      output, so it is never actually `current_exe()`'s sibling on a
//!      built system).
//!   2. the sibling of `current_exe()` — the same directory THIS process
//!      was launched from — ONLY if that file actually exists. A
//!      single-binary dev build (`cargo run -p aoide-cli`) has no `lyra`
//!      sibling; handing back a path that doesn't exist would just move
//!      the failure from "resolve" to "spawn" for no benefit, so this
//!      tier falls through instead.
//!   3. the bare name (`"aoide"`/`"lyra"`), left for `Command::spawn` to
//!      resolve off `PATH` at exec time — the tier that always exists.
//!
//! NEVER re-exec the SIBLING binary (this module's result) from inside
//! `with_stage_lock`: the lock is a `flock`, already cross-process safe,
//! but it is held per-PROCESS — a child spawned while the parent holds it
//! would deadlock waiting on a lock its own parent is still sitting on,
//! and nothing breaks that wait. Re-execing the SAME running binary
//! (`current_exe()`, the idiom `graph/spawn.rs`, `graph/permit.rs`, and
//! `server/src/a2a.rs` all use) is a different, unrelated case — none of
//! those three cross the lock either, but the reasoning doesn't transfer:
//! this module is only ever the CROSS-binary case.
//!
//! Windows has no execute bit, so the probes below ask the shell's own
//! question instead: a program is a name ending in a `command_suffixes`
//! suffix — `.exe`/`.com` (the Windows loader) or `.bat`/`.cmd` (which
//! `std::process::Command` runs itself, through `cmd.exe /c`) — tried in
//! `PATHEXT` order. Unix's suffix list is the single empty one, so every probe
//! below reduces to the `dir.join(name)` it has always been.

use std::path::{Path, PathBuf};

/// The Windows spawnable-suffix rule — pure, so this crate's tests prove it on
/// any host, not only on the Windows one that uses it.
#[cfg(any(windows, test))]
mod windows_ext {
    /// Extensions the Windows loader takes.
    const NATIVE_IMAGE_EXTENSIONS: [&str; 2] = [".exe", ".com"];
    /// Extensions `std::process::Command` runs ITSELF, as `cmd.exe /c <script>`
    /// (verified rustc 1.97.1, `std/src/sys/process/windows.rs`:
    /// `is_batch_file`/`command_prompt`/`make_bat_command_line`) — so calling
    /// one spawnable points at a real runner rather than promising an execution
    /// this crate would have to shell out for.
    const BATCH_EXTENSIONS: [&str; 2] = [".bat", ".cmd"];
    /// `cmd.exe`'s documented default `PATHEXT`, filtered to those two
    /// families — the answer when the variable is unset.
    const DEFAULT_PATHEXT: [&str; 4] = [".com", ".exe", ".bat", ".cmd"];

    /// Is `ext` (lowercased, dot included) one this crate can run?
    fn is_spawnable_extension(ext: &str) -> bool {
        NATIVE_IMAGE_EXTENSIONS.contains(&ext) || BATCH_EXTENSIONS.contains(&ext)
    }

    /// The spawnable `PATHEXT` entries, lowercased, in the shell's own order —
    /// that order IS precedence (what `cmd.exe` runs when a name is shadowed in
    /// one directory), so it is carried through, never sorted. `None` means
    /// [`DEFAULT_PATHEXT`]; blanks and dotless entries are dropped, since a
    /// shell appends an entry verbatim and `EXE` is not a suffix it would ever
    /// form. An EMPTY result is an answer, not a gap to guess into: this host
    /// declares no extension this crate can spawn.
    pub(super) fn spawnable_extensions(pathext: Option<&str>) -> Vec<String> {
        let listed: Vec<String> = match pathext {
            Some(value) => value
                .split(';')
                .map(|entry| entry.trim().to_ascii_lowercase())
                .filter(|entry| entry.starts_with('.') && is_spawnable_extension(entry))
                .collect(),
            None => DEFAULT_PATHEXT.iter().map(|ext| ext.to_string()).collect(),
        };
        let mut out: Vec<String> = Vec::new();
        for ext in listed {
            if !out.contains(&ext) {
                out.push(ext);
            }
        }
        out
    }
}

/// The suffixes a bare command name may carry on THIS platform, in precedence
/// order — the one input that differs between hosts, everything below shared.
/// Unix's list is the single empty suffix: the file name IS the command name,
/// today's behavior spelled in the same algebra rather than a second code path.
/// Windows reads `PATHEXT`. A third platform gets neither arm — a loud compile
/// failure, not a third rule.
fn command_suffixes() -> Vec<String> {
    #[cfg(windows)]
    {
        let pathext = std::env::var_os("PATHEXT").and_then(|value| value.to_str().map(str::to_string));
        windows_ext::spawnable_extensions(pathext.as_deref())
    }
    #[cfg(unix)]
    {
        vec![String::new()]
    }
}

/// Pure: the file names to try for the bare command `name`, in preference
/// order. A `name` that already carries one of `suffixes` is returned alone — a
/// caller who asked for `aoide-deploy.cmd` means that file, never a
/// doubly-suffixed one — otherwise each suffix is appended. Empty `suffixes`
/// (the Windows refusal above) yields no candidate at all.
fn candidate_names(name: &str, suffixes: &[String]) -> Vec<String> {
    let lower = name.to_ascii_lowercase();
    if suffixes.iter().any(|suffix| lower.ends_with(suffix.as_str())) {
        return vec![name.to_string()];
    }
    suffixes.iter().map(|suffix| format!("{name}{suffix}")).collect()
}

/// Pure: split a file name into the logical command name it provides and that
/// suffix's precedence rank (a lower rank wins a same-directory duplicate).
/// Case-insensitive, because Windows file names are. `None` means no spawnable
/// suffix, so this platform would not run it.
fn strip_spawnable_suffix<'a>(file_name: &'a str, suffixes: &[String]) -> Option<(&'a str, usize)> {
    let lower = file_name.to_ascii_lowercase();
    let (rank, suffix) = suffixes.iter().enumerate().find(|(_, suffix)| lower.ends_with(suffix.as_str()))?;
    file_name.get(..file_name.len() - suffix.len()).map(|stem| (stem, rank))
}

/// Pure: `file_name` past the `<bin_name>-` prefix, or `None` when it does not
/// carry it. `fold_case` is the host's own file-name rule — Windows folds
/// (`AOIDE-Deploy.EXE` belongs to `aoide-`), unix matches bytes.
fn strip_command_prefix<'a>(file_name: &'a str, prefix: &str, fold_case: bool) -> Option<&'a str> {
    let carries = if fold_case {
        file_name.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())
    } else {
        file_name.starts_with(prefix)
    };
    if !carries {
        return None;
    }
    file_name.get(prefix.len()..)
}

/// Pure tier logic: no env or filesystem reads of its own, every input
/// supplied by the caller. `exe_dir` is the directory `current_exe()`
/// resolved to (`None` if that call failed); `sibling` is that directory's
/// own FILE NAME for the sibling (`None` when it holds no such file), so the
/// tier returns a path that names something which exists — `aoide` on unix,
/// `aoide.exe` on Windows. Kept as plain arguments so this stays
/// table-testable without a real `current_exe()` or a real file on disk.
fn resolve(env_value: Option<&str>, exe_dir: Option<&Path>, sibling: Option<&str>, name: &str) -> String {
    if let Some(trimmed) = env_value.map(str::trim).filter(|v| !v.is_empty()) {
        return trimmed.to_string();
    }
    if let Some(dir) = exe_dir {
        if let Some(sibling) = sibling {
            return dir.join(sibling).to_string_lossy().into_owned();
        }
    }
    name.to_string()
}

/// This directory's own file name for the sibling binary, or `None` when it
/// holds none. The name asked for first is the one this platform's binaries
/// carry — `std::env::consts::EXE_SUFFIX`, so `aoide.exe`/`lyra.exe` on
/// Windows and the bare name on unix — with the bare name kept behind it, the
/// same order `Command::new(dir.join(name))` resolves for itself. `PATHEXT`
/// deliberately does NOT apply here: a sibling is the build sitting beside this
/// process, so an `aoide.cmd` next to it must never shadow the `aoide.exe` that
/// was actually built.
fn sibling_in(dir: &Path, name: &str) -> Option<String> {
    let native = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let found = [native.as_str(), name].into_iter().find(|candidate| dir.join(candidate).exists());
    found.map(str::to_string)
}

/// The impure wrapper shared by [`core_bin`]/[`rice_bin`]: reads the real
/// env var and `current_exe()`, does the real `exists()` stat, then hands
/// everything to [`resolve`] for the actual tier decision. The override reaches
/// `resolve` exactly as it always has — trimmed — and is then returned
/// untouched: never suffixed, never built into a shell string (a `.cmd`/`.bat`
/// override is run by `Command`, which wraps a batch file in `cmd.exe` itself).
fn resolve_bin(env_var: &str, name: &str) -> String {
    let env_value = std::env::var(env_var).ok();
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let sibling = exe_dir.as_deref().and_then(|dir| sibling_in(dir, name));
    resolve(env_value.as_deref(), exe_dir.as_deref(), sibling.as_deref(), name)
}

/// Resolve the core `aoide` binary for a cross-binary re-exec:
/// `AOIDE_CORE_BIN` env override, else `current_exe()`'s sibling named
/// `aoide` (`aoide.exe` on Windows) if it exists, else the bare name
/// `"aoide"` for `PATH` resolution at spawn time. See the module doc for the
/// with_stage_lock hazard this exists to route around, not to walk into.
pub fn core_bin() -> String {
    resolve_bin("AOIDE_CORE_BIN", "aoide")
}

/// Resolve the paint binary `lyra` for a cross-binary re-exec:
/// `AOIDE_RICE_BIN` env override, else `current_exe()`'s sibling named
/// `lyra` (`lyra.exe` on Windows) if it exists, else the bare name `"lyra"`
/// for `PATH` resolution at spawn time. See the module doc for the
/// with_stage_lock hazard this exists to route around, not to walk into.
pub fn rice_bin() -> String {
    resolve_bin("AOIDE_RICE_BIN", "lyra")
}

/// Is `name` a program discoverable on `PATH`? The proactive probe the tier
/// logic above deliberately skips: tier 3 (the bare name) is left for
/// `Command::spawn` to resolve at exec time rather than checked here, but a
/// caller that needs to know BEFORE spawning has nowhere else in the tree to
/// ask — every existing PATH-adjacent check (`aoide-secrets`' age-binary
/// probe) is a spawn-failure/ENOENT catch instead. Onboard's own lyra probe
/// (ONBOARD.md decision 3) is the first caller: `rice_bin()`'s tier-1/2
/// results are already trusted by the resolver itself (env unconditionally,
/// the sibling only after its own `exists()` check), so this only needs
/// calling on a bare-name result. `agents::on_path` is the other caller,
/// over an `AgentProfile`'s own `launch` program name.
///
/// `is_file()` alone stays the predicate — a caller whose contract is
/// "present", not "spawnable" (see [`is_executable_file`]) — but the names
/// tried are [`candidate_names`]': on unix still `dir.join(name)` alone, on
/// Windows `lyra` is `lyra.exe`.
pub fn on_path(name: &str) -> bool {
    let candidates = candidate_names(name, &command_suffixes());
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| candidates.iter().any(|c| dir.join(c).is_file())))
        .unwrap_or(false)
}

/// Is `path` a regular file this platform would run under that name? The check
/// `on_path` above deliberately skips (`is_file()` alone): a caller that means
/// to SPAWN the result must not treat a same-named non-executable file as a
/// hit. What counts as executable is [`executable_predicate`]'s.
fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if !meta.is_file() {
        return false;
    }
    executable_predicate(path, &meta)
}

/// unix: at least one execute bit, unchanged.
#[cfg(unix)]
fn executable_predicate(_path: &Path, meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

/// Windows: the shell's own predicate (see [`windows_ext`]), which is also the
/// question `Command` asks when it resolves a name to a file — the only reason
/// it can stand in for the execute bit NTFS does not have.
#[cfg(windows)]
fn executable_predicate(path: &Path, _meta: &std::fs::Metadata) -> bool {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| strip_spawnable_suffix(file_name, &command_suffixes()).is_some())
}

/// Resolve `name` to an absolute path if a SPAWNABLE file by that name sits
/// on `PATH` — task #138's external-subcommand probe
/// (`aoide_protocol::door::run`) needs both the resolved path (to spawn) and
/// true executability (a stray non-executable `aoide-foo` must fall through
/// to the ordinary unknown-command error, same as a miss). Kept as its own
/// walk rather than widening `on_path`'s contract: `on_path`'s existing
/// callers (`agents::on_path`, onboard's own lyra probe) and its own
/// regression test intentionally accept a non-executable same-named file as
/// "found," and that must not change under them.
///
/// The duplicate rule is the one a shell uses, and [`discover_external`] uses
/// the same one: the FIRST `PATH` directory holding any candidate wins, and
/// within one directory the earliest suffix in `PATHEXT` order wins. On unix
/// there is a single candidate — `dir.join(name)` — so both halves are the
/// walk this has always been.
pub fn resolve_executable_on_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let candidates = candidate_names(name, &command_suffixes());
    for dir in std::env::split_paths(&paths) {
        for candidate in &candidates {
            let path = dir.join(candidate);
            if is_executable_file(&path) {
                return Some(path);
            }
        }
    }
    None
}

/// Every `<bin_name>-<name>` program found on `PATH` — the plugin inventory
/// `schema --json`'s additive `external` key (CONTRACTS.md §3) and `--help`'s
/// own trailing external section (`door::usage_root`) both list. Walks each
/// `PATH` directory in order, keeping the FIRST match for a given name — a
/// later directory shadowing an earlier one on `PATH` never overrides what a
/// shell would actually run — and inside one directory the earliest suffix in
/// `PATHEXT` order wins a duplicate (`aoide-foo.com` before `aoide-foo.exe`
/// on a stock host, as `cmd.exe` would pick). Returns `(name, resolved
/// absolute path)` pairs sorted by name (directory read order is not
/// guaranteed, and both consumers need a deterministic document).
///
/// The name is the LOGICAL external command name, suffix stripped:
/// `aoide-deploy.exe` registers as `deploy` — what `schema --json`'s
/// `external[].name`/`command` promise, and what [`resolve_executable_on_path`]
/// is asked for when a caller types `aoide deploy`. The prefix and the
/// duplicate key fold case exactly where the filesystem does (Windows, via
/// `cfg!(windows)`; on unix both stay byte-for-byte), so an `AOIDE-Foo.EXE`
/// this probe can spawn is also a name this list reports, once.
pub fn discover_external(bin_name: &str) -> Vec<(String, PathBuf)> {
    let Some(paths) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let prefix = format!("{bin_name}-");
    let suffixes = command_suffixes();
    let fold_case = cfg!(windows);
    let mut found: std::collections::BTreeMap<String, PathBuf> = std::collections::BTreeMap::new();
    for dir in std::env::split_paths(&paths) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut here: std::collections::BTreeMap<String, (usize, PathBuf)> = std::collections::BTreeMap::new();
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(str::to_string) else { continue };
            let Some(rest) = strip_command_prefix(&file_name, &prefix, fold_case) else { continue };
            let Some((name, rank)) = strip_spawnable_suffix(rest, &suffixes) else { continue };
            if name.is_empty() {
                continue;
            }
            let path = entry.path();
            if !is_executable_file(&path) {
                continue;
            }
            let key = if fold_case { name.to_ascii_lowercase() } else { name.to_string() };
            match here.entry(key) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert((rank, path));
                }
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    if rank < slot.get().0 {
                        slot.insert((rank, path));
                    }
                }
            }
        }
        for (name, (_rank, path)) in here {
            found.entry(name).or_insert(path);
        }
    }
    found.into_iter().collect()
}

/// Shared `PATH`-mutation lock for this crate's own PATH-touching tests.
/// `bin::tests` and `agents::tests` (`agents::on_path` delegates straight
/// into the function above) both mutate the real `PATH` env var, so they
/// share ONE mutex rather than each guarding a different one
/// (crates/AGENTS.md's "process-global env... must share ONE mutex or they
/// race") — `door::tests` (the external-command probe) and `registry::tests`
/// (the `external` schema key) share it too, same reason. `pub(crate)`, not
/// module-local: the whole reason this lives
/// outside `mod tests` below.
#[cfg(test)]
pub(crate) fn path_test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}

/// Mark a just-written file executable (owner bit only, `0o755` — every
/// caller's file is scratch, never shared). `pub(crate)`, alongside
/// [`path_test_lock`], so `door::tests` and `registry::tests` can build a
/// fake plugin without reimplementing the `PermissionsExt` dance. unix-only
/// by construction: there is no bit to set on Windows, where a test makes a
/// plugin spawnable by NAMING it `aoide-foo.exe` instead.
#[cfg(all(test, unix))]
pub(crate) fn mark_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Put a saved process-global env value back. Windows' tests mutate
    /// `PATHEXT` alongside `PATH` (the unix ones never read it), so they need
    /// a two-variable restore; both hold `path_test_lock` for the whole test.
    #[cfg(windows)]
    fn restore_var(name: &str, saved: Option<std::ffi::OsString>) {
        match saved {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    /// One row per tier decision the resolver has to make. No real env,
    /// no real filesystem, no real exec anywhere in this table —
    /// `resolve` takes every input as a plain argument, and `sibling` is the
    /// sibling's FILE NAME (`aoide` on unix, `aoide.exe` on Windows), which
    /// the sibling tier only ever supplies after finding it.
    struct Case {
        label: &'static str,
        env_value: Option<&'static str>,
        exe_dir: Option<&'static str>,
        sibling: Option<&'static str>,
        name: &'static str,
        expect: &'static str,
    }

    #[test]
    fn tiers_resolve_in_order() {
        let cases = [
            Case {
                label: "env wins even when a sibling exists",
                env_value: Some("/opt/custom/aoide"),
                exe_dir: Some("/usr/bin"),
                sibling: Some("aoide"),
                name: "aoide",
                expect: "/opt/custom/aoide",
            },
            Case {
                label: "env wins over the bare-name fallback too",
                env_value: Some("/opt/custom/lyra"),
                exe_dir: None,
                sibling: None,
                name: "lyra",
                expect: "/opt/custom/lyra",
            },
            Case {
                label: "a blank env value is treated as unset",
                env_value: Some("   "),
                exe_dir: Some("/usr/bin"),
                sibling: Some("aoide"),
                name: "aoide",
                expect: "/usr/bin/aoide",
            },
            Case {
                label: "sibling used only when it actually exists",
                env_value: None,
                exe_dir: Some("/usr/bin"),
                sibling: Some("lyra"),
                name: "lyra",
                expect: "/usr/bin/lyra",
            },
            Case {
                label: "the sibling's own file name is what comes back (aoide.exe, not aoide)",
                env_value: None,
                exe_dir: Some("/usr/bin"),
                sibling: Some("aoide.exe"),
                name: "aoide",
                expect: "/usr/bin/aoide.exe",
            },
            Case {
                label: "sibling absent falls through to the bare name",
                env_value: None,
                exe_dir: Some("/usr/bin"),
                sibling: None,
                name: "lyra",
                expect: "lyra",
            },
            Case {
                label: "no exe dir at all falls through to the bare name",
                env_value: None,
                exe_dir: None,
                sibling: None,
                name: "aoide",
                expect: "aoide",
            },
        ];

        for c in cases {
            let exe_dir = c.exe_dir.map(Path::new);
            let got = resolve(c.env_value, exe_dir, c.sibling, c.name);
            assert_eq!(got, c.expect, "{}", c.label);
        }
    }

    /// The Windows suffix rule, proven on whatever host runs this suite:
    /// `PATHEXT`'s order IS precedence, matching is case-insensitive, and an
    /// entry with no runner behind it is refused rather than reported
    /// spawnable.
    #[test]
    fn pathext_order_is_precedence_and_script_types_without_a_runner_are_refused() {
        let cases: [(Option<&str>, &[&str]); 7] = [
            (None, &[".com", ".exe", ".bat", ".cmd"]),
            (Some(".EXE;.CMD"), &[".exe", ".cmd"]),
            (Some(".CMD;.EXE"), &[".cmd", ".exe"]),
            (Some(";.Exe;;.EXE;"), &[".exe"]),
            (Some(".PS1;.VBS;.JS"), &[]),
            (Some("EXE"), &[]),
            (Some(""), &[]),
        ];
        for (pathext, expect) in cases {
            let expected: Vec<String> = expect.iter().map(|ext| ext.to_string()).collect();
            assert_eq!(
                windows_ext::spawnable_extensions(pathext),
                expected,
                "PATHEXT = {pathext:?}"
            );
        }
    }

    #[test]
    fn candidate_names_append_each_suffix_in_order_and_leave_an_explicit_one_alone() {
        let stock = windows_ext::spawnable_extensions(Some(".COM;.EXE;.BAT;.CMD"));
        assert_eq!(
            candidate_names("aoide-deploy", &stock),
            vec!["aoide-deploy.com", "aoide-deploy.exe", "aoide-deploy.bat", "aoide-deploy.cmd"]
        );
        assert_eq!(
            candidate_names("aoide-deploy.exe", &stock),
            vec!["aoide-deploy.exe"],
            "a name that already carries a spawnable suffix is taken as named, never suffixed twice"
        );
        assert_eq!(candidate_names("aoide-deploy.CMD", &stock), vec!["aoide-deploy.CMD"], "case-insensitive");
        assert_eq!(
            candidate_names("aoide-deploy.ps1", &windows_ext::spawnable_extensions(Some(".PS1"))),
            Vec::<String>::new(),
            "a host declaring nothing this crate can run has no candidate at all"
        );
        // The unix list is the single empty suffix, so the same rule spells
        // today's `dir.join(name)` exactly — dotted names included.
        assert_eq!(candidate_names("lyra", &[String::new()]), vec!["lyra"]);
        assert_eq!(candidate_names("lyra.exe", &[String::new()]), vec!["lyra.exe"]);
        #[cfg(unix)]
        assert_eq!(command_suffixes(), vec![String::new()], "unix appends nothing, as it always has");
    }

    #[test]
    fn strip_spawnable_suffix_names_the_logical_command_and_ranks_the_duplicate() {
        let stock = windows_ext::spawnable_extensions(Some(".EXE;.BAT;.CMD"));
        assert_eq!(strip_spawnable_suffix("deploy.exe", &stock), Some(("deploy", 0)));
        assert_eq!(strip_spawnable_suffix("DEPLOY.Cmd", &stock), Some(("DEPLOY", 2)));
        assert_eq!(strip_spawnable_suffix("deploy.exe.bat", &stock), Some(("deploy.exe", 1)));
        assert_eq!(strip_spawnable_suffix("deploy.ps1", &stock), None);
        assert_eq!(strip_spawnable_suffix("deploy", &stock), None);
        // unix: nothing is stripped, so a dotted file name IS the name.
        assert_eq!(strip_spawnable_suffix("deploy.exe", &[String::new()]), Some(("deploy.exe", 0)));
    }

    #[test]
    fn strip_command_prefix_folds_the_bin_prefix_only_where_the_filesystem_does() {
        assert_eq!(strip_command_prefix("aoide-deploy.exe", "aoide-", false), Some("deploy.exe"));
        assert_eq!(strip_command_prefix("AOIDE-Deploy.EXE", "aoide-", false), None, "unix matches bytes");
        assert_eq!(strip_command_prefix("AOIDE-Deploy.EXE", "aoide-", true), Some("Deploy.EXE"));
        assert_eq!(strip_command_prefix("lyra-deploy", "aoide-", true), None);
    }

    /// unix-only: the scoped file is extensionless, which IS a program there
    /// and is not one on Windows (`windows_on_path_finds_the_exe_and_refuses_
    /// the_bare_name` is that half).
    #[cfg(unix)]
    #[test]
    fn on_path_finds_a_program_in_a_scoped_path_and_misses_a_name_that_is_not_there() {
        let _guard = path_test_lock().lock().unwrap();
        let saved = std::env::var_os("PATH");
        let dir = std::env::temp_dir().join(format!("aoide_bin_on_path_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("definitely-there"), "").unwrap();
        std::env::set_var("PATH", &dir);

        assert!(on_path("definitely-there"));
        assert!(!on_path("definitely-not-there"));

        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_path_is_false_when_path_is_unset() {
        let _guard = path_test_lock().lock().unwrap();
        let saved = std::env::var_os("PATH");
        std::env::remove_var("PATH");

        assert!(!on_path("anything"));

        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
    }

    /// unix-only: the executable BIT is the predicate, which is exactly what
    /// Windows replaces with a suffix.
    #[cfg(unix)]
    #[test]
    fn resolve_executable_on_path_requires_the_executable_bit() {
        let _guard = path_test_lock().lock().unwrap();
        let saved = std::env::var_os("PATH");
        let dir = std::env::temp_dir().join(format!("aoide_bin_resolve_exec_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plain-file"), "").unwrap();
        std::fs::write(dir.join("real-plugin"), "#!/bin/sh\n").unwrap();
        mark_executable(&dir.join("real-plugin"));
        std::env::set_var("PATH", &dir);

        assert_eq!(resolve_executable_on_path("real-plugin"), Some(dir.join("real-plugin")));
        assert_eq!(
            resolve_executable_on_path("plain-file"),
            None,
            "a same-named non-executable file must not count as a hit"
        );
        assert_eq!(resolve_executable_on_path("nowhere"), None);

        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows: the executable bit's stand-in is the suffix, so the same
    /// scoped-PATH probe answers for `real-plugin.exe` and refuses both the
    /// extensionless file and a script type no `Command` runner backs.
    #[cfg(windows)]
    #[test]
    fn windows_resolve_executable_on_path_requires_a_spawnable_suffix() {
        let _guard = path_test_lock().lock().unwrap();
        let (saved_path, saved_pathext) = (std::env::var_os("PATH"), std::env::var_os("PATHEXT"));
        let dir = std::env::temp_dir().join(format!("aoide_bin_win_resolve_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("real-plugin.exe"), "").unwrap();
        std::fs::write(dir.join("plain-file"), "").unwrap();
        std::fs::write(dir.join("script-plugin.ps1"), "").unwrap();
        std::env::set_var("PATH", &dir);
        std::env::set_var("PATHEXT", ".EXE;.PS1");

        assert_eq!(resolve_executable_on_path("real-plugin"), Some(dir.join("real-plugin.exe")));
        assert_eq!(
            resolve_executable_on_path("plain-file"),
            None,
            "no suffix is no program, the unix non-executable file's counterpart"
        );
        assert_eq!(
            resolve_executable_on_path("script-plugin"),
            None,
            "a .ps1 has no runner in std, so it is not spawnable by name here"
        );
        assert_eq!(resolve_executable_on_path("nowhere"), None);

        restore_var("PATH", saved_path);
        restore_var("PATHEXT", saved_pathext);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows, all three surfaces at once: an earlier `PATH` directory wins a
    /// duplicate, `PATHEXT` order decides inside one directory, the exposed
    /// name has its executable suffix (and its case) folded away, and a host
    /// declaring nothing runnable discovers nothing.
    #[cfg(windows)]
    #[test]
    fn windows_discovery_surfaces_agree_on_the_duplicate_and_on_the_logical_name() {
        let _guard = path_test_lock().lock().unwrap();
        let (saved_path, saved_pathext) = (std::env::var_os("PATH"), std::env::var_os("PATHEXT"));
        let first = std::env::temp_dir().join(format!("aoide_bin_win_first_{}", std::process::id()));
        let second = std::env::temp_dir().join(format!("aoide_bin_win_second_{}", std::process::id()));
        for dir in [&first, &second] {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).unwrap();
        }
        // Same logical plugin, twice, with the later suffix and the later
        // directory both losing.
        std::fs::write(first.join("AOIDE-Deploy.cmd"), "").unwrap();
        std::fs::write(second.join("aoide-deploy.exe"), "").unwrap();
        std::fs::write(first.join("aoide-zap.ps1"), "").unwrap();
        std::env::set_var("PATH", std::env::join_paths([&first, &second]).unwrap());

        std::env::set_var("PATHEXT", ".EXE;.CMD;.PS1");
        assert_eq!(
            resolve_executable_on_path("aoide-deploy"),
            Some(first.join("AOIDE-Deploy.cmd")),
            "the first PATH directory wins even when its suffix ranks later"
        );
        assert_eq!(
            discover_external("aoide"),
            vec![("deploy".to_string(), first.join("AOIDE-Deploy.cmd"))],
            "one entry per logical name, suffix and case folded away"
        );

        // Drop the winner: the next directory, then the suffix order, decides.
        std::fs::remove_file(first.join("AOIDE-Deploy.cmd")).unwrap();
        assert_eq!(resolve_executable_on_path("aoide-deploy"), Some(second.join("aoide-deploy.exe")));
        assert_eq!(discover_external("aoide"), vec![("deploy".to_string(), second.join("aoide-deploy.exe"))]);

        // A host whose PATHEXT names nothing this crate can run refuses.
        std::env::set_var("PATHEXT", ".PS1");
        assert_eq!(resolve_executable_on_path("aoide-deploy"), None);
        assert!(discover_external("aoide").is_empty());

        restore_var("PATH", saved_path);
        restore_var("PATHEXT", saved_pathext);
        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
    }

    /// Windows: `on_path` keeps its own weaker predicate (`is_file`), on the
    /// same candidates — so the bare extensionless file is not a program, and
    /// the `.exe` beside it is.
    #[cfg(windows)]
    #[test]
    fn windows_on_path_finds_the_exe_and_refuses_the_bare_name() {
        let _guard = path_test_lock().lock().unwrap();
        let (saved_path, saved_pathext) = (std::env::var_os("PATH"), std::env::var_os("PATHEXT"));
        let dir = std::env::temp_dir().join(format!("aoide_bin_win_on_path_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("definitely-there.exe"), "").unwrap();
        std::fs::write(dir.join("definitely-bare"), "").unwrap();
        std::env::set_var("PATH", &dir);
        std::env::set_var("PATHEXT", ".EXE;.CMD");

        assert!(on_path("definitely-there"));
        assert!(!on_path("definitely-bare"), "an extensionless file is not a program a shell would run by name");
        assert!(!on_path("definitely-not-there"));

        restore_var("PATH", saved_path);
        restore_var("PATHEXT", saved_pathext);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows: the sibling tier is `EXE_SUFFIX`, not the plugin rule — an
    /// `aoide.cmd` beside the real `aoide.exe` must not shadow it.
    #[cfg(windows)]
    #[test]
    fn windows_sibling_resolution_names_the_exe_not_a_cmd_beside_it() {
        let dir = std::env::temp_dir().join(format!("aoide_bin_win_sibling_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("aoide.cmd"), "").unwrap();
        std::fs::write(dir.join("lyra.exe"), "").unwrap();

        assert_eq!(sibling_in(&dir, "lyra"), Some("lyra.exe".to_string()));
        assert_eq!(sibling_in(&dir, "aoide"), Some("aoide.cmd".to_string()), "the only sibling present is the one used");
        assert_eq!(
            resolve(None, Some(&dir), sibling_in(&dir, "lyra").as_deref(), "lyra"),
            dir.join("lyra.exe").to_string_lossy()
        );
        assert_eq!(sibling_in(&dir, "conductor"), None, "no sibling at all still falls through to the bare name");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// unix-only: extensionless plugins are programs there, and only the
    /// execute bit decides (`windows_discovery_surfaces_agree...` is the
    /// Windows half, where the suffix decides instead).
    #[cfg(unix)]
    #[test]
    fn discover_external_finds_every_prefixed_executable_and_skips_the_rest() {
        let _guard = path_test_lock().lock().unwrap();
        let saved = std::env::var_os("PATH");
        let dir = std::env::temp_dir().join(format!("aoide_bin_discover_external_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Two real plugins, in a deliberately non-alphabetical write order.
        std::fs::write(dir.join("aoide-zebra"), "").unwrap();
        mark_executable(&dir.join("aoide-zebra"));
        std::fs::write(dir.join("aoide-deploy"), "").unwrap();
        mark_executable(&dir.join("aoide-deploy"));
        // A non-executable same-prefix file: must not appear.
        std::fs::write(dir.join("aoide-unexecutable"), "").unwrap();
        // An unrelated executable and a bare "aoide-": neither is a plugin.
        std::fs::write(dir.join("unrelated"), "").unwrap();
        mark_executable(&dir.join("unrelated"));
        std::fs::write(dir.join("aoide-"), "").unwrap();
        mark_executable(&dir.join("aoide-"));
        std::env::set_var("PATH", &dir);

        let got = discover_external("aoide");
        assert_eq!(
            got,
            vec![("deploy".to_string(), dir.join("aoide-deploy")), ("zebra".to_string(), dir.join("aoide-zebra"))],
            "sorted by name, non-executable/unrelated/bare-prefix entries excluded"
        );

        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discover_external_is_empty_when_path_is_unset() {
        let _guard = path_test_lock().lock().unwrap();
        let saved = std::env::var_os("PATH");
        std::env::remove_var("PATH");

        assert!(discover_external("aoide").is_empty());

        match saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
    }

    #[test]
    fn core_bin_and_rice_bin_resolve_to_different_env_vars() {
        // No filesystem/env stubbing here (the process env is process-wide
        // and races under parallel tests) — this only checks the two public
        // entries stay wired to their own env var / bare name and never
        // collide, using the pure `resolve` core directly rather than the
        // real `std::env::var`/`current_exe` the wrappers call.
        assert_eq!(resolve(None, None, None, "aoide"), "aoide");
        assert_eq!(resolve(None, None, None, "lyra"), "lyra");
        assert_ne!(resolve(None, None, None, "aoide"), resolve(None, None, None, "lyra"));
    }
}
