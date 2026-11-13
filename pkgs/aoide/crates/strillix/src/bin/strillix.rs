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
    let code = strillix::run_strillix(&argv);
    std::process::exit(code);
}
