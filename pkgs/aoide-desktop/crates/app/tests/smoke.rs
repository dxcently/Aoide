//! The headless smoke run. Each scenario file under `smoke/` starts the app
//! under cage, in a headless session of its own. The harness waits for the
//! app's `ready` file, takes a screenshot with grim, and ends cage with SIGTERM.
//! Every process starts through the exec crate's test starter, and cage is
//! the only compositor the app can reach.

use std::ffi::OsString;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use aoide_desktop_shell::exec::test_doors;
use serde::Deserialize;

/// How long the app has to write `ready` once cage starts.
const READY_WAIT: Duration = Duration::from_secs(60);
/// How often the harness checks for `ready`.
const POLL: Duration = Duration::from_millis(100);

/// A scenario file. Its confirm is what the app's dialog answers.
#[derive(Deserialize)]
struct Scenario {
    confirm: bool,
}

/// Every scenario file under `smoke/`, by name, read and parsed. A file that
/// cannot be read or parsed fails the run.
fn scenarios() -> Result<Vec<(String, String)>, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("smoke");
    let mut found = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str::<Scenario>(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        found.push((name, text));
    }
    found.sort();
    Ok(found)
}

/// The GL and EGL paths the dev shell names for the VM, which cage's env is given
/// from the harness's own environment. Nothing else is forwarded.
const FROM_DEV_SHELL: [&str; 3] = ["__EGL_VENDOR_LIBRARY_DIRS", "LIBGL_DRIVERS_PATH", "GBM_BACKENDS_PATH"];

/// One of the dev shell's variables, passed through. Unset is a failure that names it.
fn from_dev_shell(name: &str) -> Result<(OsString, OsString), String> {
    std::env::var_os(name)
        .map(|value| var(name, value))
        .ok_or_else(|| format!("{name} is not set: the dev shell must name it"))
}

/// A program on the dev shell's PATH, by its absolute path.
fn on_path(name: &str) -> Result<PathBuf, String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("{name} is not on the dev shell's PATH"))
}

/// Stdout and stderr, both appended to one file.
fn log_pair(path: &Path) -> Result<(Stdio, Stdio), String> {
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let twin = file.try_clone().map_err(|e| e.to_string())?;
    Ok((Stdio::from(file), Stdio::from(twin)))
}

fn var(name: &str, value: impl Into<OsString>) -> (OsString, OsString) {
    (OsString::from(name), value.into())
}

/// Whether a PPM image's pixels are all one byte value. The header is `P6`,
/// the width, the height and the maximum, each a token; one whitespace byte
/// follows the maximum, then the pixels.
fn one_colour(ppm: &[u8]) -> Result<bool, String> {
    let mut at = 0;
    let mut tokens: Vec<&[u8]> = Vec::new();
    while tokens.len() < 4 {
        while at < ppm.len() && ppm[at].is_ascii_whitespace() {
            at += 1;
        }
        let start = at;
        while at < ppm.len() && !ppm[at].is_ascii_whitespace() {
            at += 1;
        }
        if start == at {
            return Err("the screenshot is not a PPM image".to_owned());
        }
        tokens.push(&ppm[start..at]);
    }
    if tokens[0] != &b"P6"[..] {
        return Err("the screenshot is not a P6 PPM image".to_owned());
    }
    let pixels = &ppm[at + 1..];
    let first = *pixels.first().ok_or("the screenshot has no pixels")?;
    Ok(pixels.iter().all(|b| *b == first))
}

/// Every path under `dir`, relative to it, sorted: what the app left in its HOME.
fn listing(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(here) = pending.pop() {
        let Ok(entries) = fs::read_dir(&here) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            out.push(path.strip_prefix(dir).unwrap_or(&path).display().to_string());
        }
    }
    out.sort();
    out
}

/// Ends cage by SIGTERM, through its recorded pid, and waits on it. Never a kill.
fn end_cage(cage: &mut Child) -> Result<ExitStatus, String> {
    if let Some(status) = cage.try_wait().map_err(|e| e.to_string())? {
        return Ok(status);
    }
    test_doors::terminate(cage.id()).map_err(|e| format!("cage did not take SIGTERM: {e}"))?;
    cage.wait().map_err(|e| e.to_string())
}

/// The parent of `pid`, from its stat line; the command name in it may hold spaces.
fn parent_of(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_name = &stat[stat.rfind(')')? + 2..];
    after_name.split_whitespace().nth(1)?.parse().ok()
}

/// The page process must run in a PID namespace of its own, which only WebKit's
/// sandbox gives it. The app is cage's child; every WebKitWebProcess is checked.
fn sandbox_check(cage: u32) -> Result<(), String> {
    let mut app = None;
    let mut pages = Vec::new();
    for entry in fs::read_dir("/proc").map_err(|e| e.to_string())?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(exe) = fs::read_link(entry.path().join("exe")) else { continue };
        if exe.ends_with("aoide-desktop") && parent_of(pid) == Some(cage) {
            app = Some(pid);
        }
        if exe.ends_with("WebKitWebProcess") {
            pages.push(pid);
        }
    }
    let app = app.ok_or("the app is not cage's child")?;
    if pages.is_empty() {
        return Err("no WebKitWebProcess runs under the app".to_owned());
    }
    let app_ns = fs::read_link(format!("/proc/{app}/ns/pid")).map_err(|e| e.to_string())?;
    for page in pages {
        let page_ns = fs::read_link(format!("/proc/{page}/ns/pid")).map_err(|e| e.to_string())?;
        if page_ns == app_ns {
            return Err(format!(
                "WebKitWebProcess (pid {page}) shares the app's PID namespace: WebKit's sandbox is off"
            ));
        }
    }
    Ok(())
}

/// Runs one scenario and returns what it found, or why it failed.
fn run_scenario(name: &str, text: &str, shots: &Path) -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("smk-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (home, run, aoide, smoke) = (root.join("home"), root.join("run"), root.join("aoide"), root.join("smoke"));
    for dir in [&home, &aoide, &smoke] {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    fs::create_dir_all(&run).map_err(|e| e.to_string())?;
    fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    fs::write(smoke.join("scenario.json"), text).map_err(|e| e.to_string())?;

    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut env = vec![
        var("PATH", path),
        var("HOME", &home),
        var("XDG_RUNTIME_DIR", &run),
        var("AOIDE_ROOT", &aoide),
        var("WLR_BACKENDS", "headless"),
        var("WLR_LIBINPUT_NO_DEVICES", "1"),
        var("WLR_RENDERER", "pixman"),
        var("NO_AT_BRIDGE", "1"),
        var("GDK_BACKEND", "wayland"),
        var("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
        var("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        var("LIBGL_ALWAYS_SOFTWARE", "1"),
    ];
    for name in FROM_DEV_SHELL {
        env.push(from_dev_shell(name)?);
    }
    let cage_bin = on_path("cage")?;
    let grim_bin = on_path("grim")?;
    let app = PathBuf::from(env!("CARGO_BIN_EXE_aoide-desktop"));
    let args = [OsString::from("--"), app.into_os_string(), OsString::from("--smoke"), smoke.clone().into_os_string()];
    let (out, err) = log_pair(&root.join("cage.log"))?;
    let mut cage = test_doors::start(&cage_bin, &args, &env, out, err).map_err(|e| format!("cage did not start: {e}"))?;

    let shot = screenshot(&mut cage, &run, &smoke, &env, &grim_bin, &root, name, shots);
    let sandbox = if shot.is_ok() { sandbox_check(cage.id()) } else { Ok(()) };
    let ended = end_cage(&mut cage);
    let _ = fs::copy(root.join("cage.log"), shots.join(format!("{name}.cage.log")));
    let status = ended?;
    let cage_ok = status.code() == Some(0) || status.signal() == Some(15);
    let scenario: Scenario = serde_json::from_str(text).map_err(|e| format!("{name}: {e}"))?;
    println!("scenario {name}: confirm {}, cage ended {status}", scenario.confirm);
    println!("scenario {name}: HOME holds {:?}", listing(&home));
    shot?;
    sandbox?;
    if !cage_ok {
        return Err(format!("cage did not end on its SIGTERM: {status}"));
    }
    Ok(())
}

/// Waits for `ready`, then takes the screenshot the scenario is judged by.
#[allow(clippy::too_many_arguments)]
fn screenshot(
    cage: &mut Child,
    run: &Path,
    smoke: &Path,
    env: &[(OsString, OsString)],
    grim: &Path,
    root: &Path,
    name: &str,
    shots: &Path,
) -> Result<(), String> {
    let started = Instant::now();
    while !smoke.join("ready").exists() {
        if let Some(status) = cage.try_wait().map_err(|e| e.to_string())? {
            let log = fs::read_to_string(root.join("cage.log")).unwrap_or_default();
            return Err(format!("cage ended before ready: {status}\n{log}"));
        }
        if started.elapsed() > READY_WAIT {
            let log = fs::read_to_string(root.join("cage.log")).unwrap_or_default();
            let tail: String = log.chars().rev().take(4000).collect::<Vec<_>>().into_iter().rev().collect();
            return Err(format!("ready did not appear within 60 s; cage.log ends:\n{tail}"));
        }
        std::thread::sleep(POLL);
    }
    let mut grab_env = env.to_vec();
    grab_env.push(var("WAYLAND_DISPLAY", "wayland-0"));
    if !run.join("wayland-0").exists() {
        return Err("cage made no wayland-0 socket".to_owned());
    }
    let check = root.join("check.ppm");
    grab(grim, &grab_env, &[OsString::from("-t"), OsString::from("ppm"), check.clone().into_os_string()], root)?;
    let png = shots.join(format!("{name}.png"));
    grab(grim, &grab_env, &[png.clone().into_os_string()], root)?;
    let bytes = fs::read(&check).map_err(|e| e.to_string())?;
    if one_colour(&bytes)? {
        return Err("the screenshot is one flat colour".to_owned());
    }
    println!("scenario {name}: ready, screenshot {}", png.display());
    Ok(())
}

/// Runs grim with `args` and waits for it.
fn grab(grim: &Path, env: &[(OsString, OsString)], args: &[OsString], root: &Path) -> Result<(), String> {
    let (out, err) = log_pair(&root.join("grim.log"))?;
    let mut child = test_doors::start(grim, args, env, out, err).map_err(|e| format!("grim did not start: {e}"))?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("grim failed: {status}"))
    }
}

#[test]
fn the_smoke_scenarios_run_under_cage() {
    let shots = std::env::var_os("AOIDE_SMOKE_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("smoke-shots"));
    fs::create_dir_all(&shots).expect("the screenshot directory is made");
    let scenarios = scenarios().expect("every scenario file is read");
    assert!(!scenarios.is_empty(), "smoke/ holds at least one scenario");
    let mut failed = Vec::new();
    for (name, text) in &scenarios {
        if let Err(e) = run_scenario(name, text, &shots) {
            failed.push(format!("{name}: {e}"));
        }
    }
    assert!(failed.is_empty(), "smoke scenarios failed:\n{}", failed.join("\n"));
}

#[test]
fn one_colour_tells_a_flat_page_from_a_drawn_one() {
    assert!(one_colour(b"P6\n2 1\n255\n\x01\x01\x01\x01\x01\x01").expect("a flat PPM parses"));
    assert!(!one_colour(b"P6\n2 1\n255\n\x01\x01\x01\x02\x01\x01").expect("a drawn PPM parses"));
    assert!(one_colour(b"P5\n2 1\n255\n\x01\x01").is_err());
}
