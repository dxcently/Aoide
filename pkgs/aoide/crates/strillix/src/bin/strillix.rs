//! The `strillix` binary — the graphical/rice CLI trunk (P-A4).

fn main() {
    // `strillix preview tree | head` must end quietly when `head` closes the
    // pipe (SIGPIPE's default), not panic on a failed stdout write.
    // SAFETY: resets one signal disposition to its default before any
    // thread exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    // One-shot, idempotent `~/Aoide` → `$AOIDE_ROOT` migration (L-C2, task
    // #107) — see `aoide_storage::fs::root`'s own doc for why this runs
    // here, explicitly, rather than hanging off a path getter. A moved
    // `state/` that cannot be locked owner-only stops the run here.
    if let Err(e) = aoide_storage::fs::migrate_root_once() {
        eprintln!("strillix: {e}");
        std::process::exit(1);
    }

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = strillix::run_strillix(&argv);
    std::process::exit(code);
}
