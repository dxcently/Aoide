//! `shellbridge`: the process that serves the desktop verbs on the shellbridge
//! socket. The socket loop is `aoide_conduct::shellbridge::run`, handed the verb
//! table this binary registers ([`crate::bridge::VERBS`]).

use aoide_protocol::output::Outcome;
use aoide_protocol::registry::{cmd, flag, Registry};
use aoide_protocol::Invocation;

fn handle_shellbridge(_inv: &Invocation) -> Outcome {
    // The Hyprland event listener runs beside the socket loop: it binds session windows through
    // the core door and clears closed ones. It returns at once off Hyprland, and never blocks
    // the accept loop.
    std::thread::spawn(crate::hypr::run_hypr_window_listener);
    let status = aoide_conduct::shellbridge::run(crate::bridge::VERBS);
    Outcome::ok("shellbridge", "shellbridge skeleton self-check complete").with_data(status)
}

/// `shellbridge`, registered in strillix's `System` group.
pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["shellbridge"],
        summary: "Run the shellbridge process: publish session/hook state to state/stage/ atomically.",
        args: [],
        flags: [flag!("run", "bool", "Run the long-lived shellbridge process.")],
        gated: false,
        implemented: true,
        handler: handle_shellbridge,
        brief: "Publish session and hook state for the shell surfaces.",
    ));
}
