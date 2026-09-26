pub mod agents;
pub mod audit;
pub mod bin;
pub mod dialog;
pub mod door;
pub mod feed;
pub mod invocation;
pub mod model;
/// The owner-only policy a native-Windows core attaches to a private file or
/// directory — one implementation for `feed`'s writer and `aoide-storage`'s
/// private-write half. Windows-only by construction: on Unix the same policy
/// is the mode bits those callers already set.
#[cfg(windows)]
pub mod owner_only;
pub mod output;
/// The native process table — snapshot, start time, liveness — for the two
/// callers that need a fact Unix reads out of `/proc`. Windows-only: the Unix
/// arm is `/proc` itself.
#[cfg(windows)]
pub mod win_proc;
pub mod pick;
pub mod policy;
pub mod registry;
pub mod state;
pub mod wire;

pub use audit::{append_audit, audit, aoide_home, audit_log_path, default_audit_log, AuditRecord, Door, EventClass};
pub use invocation::Invocation;
pub use model::context_ceiling_for_model;
pub use pick::{choose, choose_many, interactive, render_rows};
pub use policy::{Gate, GateProposal, Subscription};
pub use state::canonical_state;
