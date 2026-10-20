//! The `strillix` binary — the graphical/rice CLI trunk (P-A4).

fn main() {
    // `strillix preview tree | head` must end quietly when `head` closes the
    // pipe (SIGPIPE's default), not panic on a failed stdout write.
    // SAFETY: resets one signal disposition to its default before any
    // thread exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let verb = argv.first().map(String::as_str);
    // The rice tree's move is `strx migrate`'s own job, so the start-up hook
    // stays out of its way (`--check` must report, never move).
    if verb != Some("migrate") {
        if let Err(e) = strillix_rice::migrate::rice_root_at_start() {
            eprintln!("strillix: {e}");
            std::process::exit(1);
        }
        // Until the rice state is settled, a verb that could write into the
        // strillix root would create the second live root the move refuses.
        // Only the verbs that read nothing from it run.
        let reads_nothing = matches!(verb, None | Some("help" | "--help" | "-h" | "--version" | "-V"));
        if !reads_nothing && strillix_rice::migrate::rice_status() != strillix_rice::migrate::RiceStatus::Settled {
            eprintln!("strillix: the rice state is not settled; run `strx migrate --check`, then `strx migrate`. Nothing ran");
            std::process::exit(1);
        }
    }
    let code = strillix::run_strillix(&argv);
    std::process::exit(code);
}
