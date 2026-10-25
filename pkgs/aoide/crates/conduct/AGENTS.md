# AGENTS.md — aoide-conduct

## Native Windows: what the seams are, and what is refused by name

Everything this crate needs from a host lives behind a seam, and no call site
spells a platform of its own: the socket TYPE (`aoide_protocol::win_unix`), the
process facts Unix reads out of `/proc` (`aoide_protocol::win_proc` —
`proc_argv`, `proc_comm`, `proc_has_children` answer through it, while `proc_cwd`
has NO arm and says so, falling back to the record's stamped `cwd`), the peer
identity (`win_unix::peer_pid` + `win_proc::process_user_sid`), and
`aoide_storage::fs`'s `path_is_under` (the project-prefix question, component-wise
and verbatim-stripped), `detach`, `link_dir`, `lock_exclusive` family and
`create_new_private`.

**The PTY capability is native on BOTH hosts, one seam with an arm per host:**
`graph/pty.rs` holds `spawn_on_pty` (`libc::openpty` + `setsid` + `TIOCSCTTY` +
`dup2` on Unix; `CreatePseudoConsole` + the `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`
attribute on `CreateProcessW` on native Windows), `Pty`, `PtyChild`, `Console`,
`Inbox` and `wait_ready` — with the host differences named in that module's own
table (no signals, so no `Ended::Signal`; the deadline kill reaches the direct
child only; `foreground_pgid()` answers `0`; `hung_up()` answers `None` — the
shape the caller needs, so a host that cannot answer stamps no fact; the child's
streams are the console's own, `STARTF_USESTDHANDLES` with NULL handles and
`bInheritHandles = FALSE`; the output pipe is POLLED rather than waited on, and
so is any stdin that is not a console — a pipe's read handle is always signalled
— while a console's stdin is the one the wait itself answers; and a
settle window after the client exits that a pty does not need). `graph/conduct.rs`
multiplexes over the seam and owns the policy, and `session_conduct` runs on
both hosts. **The Windows-only tests are the ones that prove it** — the seam's
seven (output read, a typed line at the child's own stdin, a resize the child's
own `mode con` reports, the child's stdout being a console rather than our pipe,
an inbox directory bound owner-only, a failed spawn leaking no handle, and the
`ready_of_wait` table) plus the command's own runs end to end: a pseudo-console
child read/resized/typed-into/observed, a typed line at the child's stdin, a
resize the child's own `mode con` reports, **a live inbox connection delivering
bytes to the conducted child**, **a piped stdin forwarding its producer's bytes
to it**, and the whole headless command resolved `done`.
**The arithmetic, defined − gated = run**: Linux defines **976** and a ThinkChiyo
run reports **933** (both measured, the runs named in `CORE-POSIX.md`); the
43-test gap is the crate's host-gated set — **58** tests need a POSIX fixture and
**15** are Windows-only — each gate naming its own reason in place. The gates are the fixture
groups rather than a number to be trusted: a `#!/bin/sh` script or shim and its
mode (the curl transport and `sh`-based conduct
children), `fork`/`SO_PEERCRED`/`setsid` facts, `/proc` reads, `socat`-style
doors, and the SIGTERM-survivor pair — each naming its own reason in place. The
fifteen Windows-only ones are the seam's seven (`ready_of_wait`'s table, a
failed spawn's handle count, the child's stdout being a console, the inbox
directory bound owner-only, the `AF_UNIX` signal probe, command-line quoting,
the environment block), the command's seven end-to-end runs (a pseudo-console
child read/resized/typed-into/observed, a typed line at the child's stdin, a
resize the child's own `mode con` reports, **a live inbox connection delivering
bytes to the conducted child**, **a piped stdin forwarding its producer's bytes
to it**, **an interactive session over a piped stdin typing the injected line
into its child**, and the whole headless command resolved `done`)
and the pre-existing `identity.rs` peer test that runs only where
`SIO_AF_UNIX_GETPEERPID` exists. The Unix side gains one of its own: the
hung-up-stdin run (`a_hung_up_stdin_is_reported_ready_so_the_loop_latches_its_eof`,
`pty.rs`), the failing-first proof that `poll(2)`'s `POLLHUP` — never `POLLIN` —
is what keeps `producer | aoide conduct` from spinning once its producer exits.
The DETACHED, non-PTY spawn is native
(`aoide_storage::fs::detach`), and its four green tests are named in the gate
text of the tests that do need a conducted child.

A fixture that pastes a path into hand-built JSON or creates a file/dir with
`std::fs` defaults is a bug on this host, not a flake: Windows paths carry `\`
(escape them — `send`'s `jp`, or build with `serde_json`), a feed's DACL must be
owner-only, and a socket path must fit `sun_path` (use
`aoide_test_support::short_tmp`). Each of those three cost real diagnosis time
once; none of them is a style preference. One more of the same kind: a fixture
that binds a listener and waits for a delivery reads it through
`aoide_test_support`'s bounded rig (`expect_delivery`/`accept_one`/
`read_delivery`), never a hand-rolled `accept`/`read_to_end` thread — a
delivery this crate withholds (a held-pending send, a ring that never fires)
would otherwise park that thread, and under `env_lock` every test queued
behind it, instead of failing the one test with the fixture's own expectation
in the message.

- **Session actions preserve identity and scope.** Project assignment changes
  `project`, never `cwd` or ancestry. Termination is local daemon-only:
  `kill_target` first resolves the requested id to the nearest
  `parentSessionId` ancestor that is itself a dedicated conducted process
  (walking up to 32 hops, cycle-guarded, the requested id itself the first
  candidate) — the card a caller names and the process a kill actually stops
  are not always the same record. A `sub:` record never resolves and the walk
  never climbs through one (a subagent shares its executor's process rather
  than owning one: "kill its executor session instead"), and an ancestor hop
  that has already ended refuses as "the hosting terminal has already ended".
  The seal is checked exactly ONCE, by `terminate_verified` against the
  RESOLVED target, never inside the walk itself — a stale seal must surface
  its own refusal, never get pre-empted by a misleading ancestry message.
  The shared-pid refusal excludes every session in the walked chain (a wrap
  and its own hook-fed descendants legitimately share one process's pid);
  only a record OUTSIDE that chain holding the target's live pid trips it.
  Never fall back to `kill(pid)` or infer completion from successful signal
  delivery. A `SessionStart` fired outside any wrap (no attested wrap, no
  `AOIDE_SESSION_ID`) clears the `parentSessionId` a resumed record still
  carries, so a later kill refuses instead of resolving through the terminal
  that hosted the previous run.

## Invariants

- **Only an approving door may approve a gated send.** `Door::can_approve`
  (`graph/send.rs`) is the one predicate: `Cli` and `Daemon` approve, `Mcp`
  and `A2a` never do. `--yes` and the global autogate environment are read
  only past that check, so a send from a remote door parks whatever it asks
  for unless an attested rule delivers it (next bullet), and
  `pending_approve` refuses before it takes an entry off the queue: when the
  door cannot approve, when the entry has expired, when its target is gone, and
  when its target is not live (not registered yet, or ended).
  `pending_deny` refuses on the same predicate, so a door that cannot approve
  cannot empty the queue under the human either. The same predicate gates
  the other two things a human's approval answers: `session_permit` files no
  summons card for a door that cannot approve (or for a session that is not
  `awaiting`), and `mail_ring` refuses before any forward, so such a caller
  never types into a reader. The predicate is exhaustive over `Door`, so a
  new door must be decided there. The door is the invocation's own `Door`,
  set where the request arrived: a caller's claim about itself is never
  read. A routed command (`daemon_dispatch`) reaches aoided as
  `Door::Daemon`, so only the CLI door routes: `daemon_dispatch` answers
  `None` for every other door, which runs its own local path. A routed
  command must never be the approving path.
  `pkgs/aoide/crates/conduct/src/graph/send.rs`'s door tests pin each path.
- **The MCP door is attested like the CLI and approves nothing.**
  `deliver_local_with` walks the dispatching process's ancestry
  (`resolve_sender_id`, `real_attested_sender`) for a door that can approve
  (unless `--yes` or the environment already decided) and for `Door::Mcp`,
  and never for `A2a`, which runs inside aoided with no sealed ancestor. An
  approving door ends in `send_gate`, which reads `--yes` and the environment
  first. `Mcp` ends in `attested_gate`: the parent rule, then the sibling
  rule (and its opt-out), else pending, with no `--yes` and no switch to read.
  Keep it so: an attested MCP sender must never reach `may_ring` (that is
  `Door::Daemon` alone), `pending_approve`, `session_permit` or `mail_ring`,
  and an MCP send is attributed to the attested session, never to the `--from`
  it typed, which rides as `claims`. The identity is who launched the server,
  not who writes to its stdin, so a network bridge run inside a conducted
  terminal lends that terminal's rights to its remote callers. Pinned by
  `an_attested_mcp_parent_sends_to_its_own_child_and_it_delivers`,
  `an_attested_mcp_sender_to_an_unrelated_session_parks`,
  `an_unattested_mcp_send_parks`,
  `an_attested_mcp_sibling_delivers_and_the_opt_out_parks`,
  `an_attested_mcp_send_still_cannot_approve_with_yes_or_the_autogate_env`,
  `an_attested_mcp_caller_still_cannot_approve_permit_or_ring`,
  `the_a2a_door_parks_for_the_parent_a_resolver_would_name_and_the_cli_delivers`
  and, through the real resolver,
  `the_real_resolver_attests_an_mcp_send_by_the_callers_sealed_ancestor`.
- **An MCP read is clipped to the caller's own subtree.** `read_scope`
  (`graph/send.rs`) gives the set the caller may see: its attested session
  (`real_attested_sender`, never a claimed id) and every session under it by
  `attested_parent_of` edges. `in_read_scope` tests membership. An unattested
  caller gets the empty set and sees nothing. Off MCP the scope is `None` and
  nothing is clipped. A forged `parent` without the spawn stamp adds nothing.
  Each MCP reader cuts to the scope in its own way:
  - The roster (`collect_roster`) and `session find` (`load_index`) keep the
    scoped rows, name no remote parent (an ancestor on another node), and
    carry no mail facts: a mailbox can be shared with a sibling outside the
    scope, so a row's mail facts stay withheld. The mail reads are scoped
    instead by `graph::mail_reads` (below).
  - `graph::mail_reads` answers `mail`, `mail read`, `mail show` and `mail
    mark` for the MCP door only. Off MCP it passes no scope and the env reader,
    so the CLI is unchanged. On MCP it takes `read_scope` and the attested
    session as the reader, refuses an unattested caller, and hands the handlers
    to `aoide-client`, which keeps the read logic. Only this module may register
    them: `commands/graph.rs` calls `aoide-client`'s `register_mail_names` and
    `register_mail_reads` with these handlers; no other caller may do so.
    Session readers key as `session:<id>` in storage, never as a bare id.
  - The graph (`view_as`) keeps the scoped sessions and hooks, and draws only
    the projects a scoped session resolves to, with no path, root or host. It
    folds no mesh node (no url, no far-node cache) and names no remote parent.
    `build_graph_as` and `render_as` take the `reveal` switch that does this.
  - `session watch` (`session_watch_as`) cuts its candidates to the scope
    before it resolves, so an out-of-scope id, tail or petname refuses with
    the text an unknown id gets, and never appears in an ambiguity list. A
    `node/<query>` target is refused the same way, with no fetch.
  Pinned by `an_mcp_caller_sees_its_own_attested_subtree_and_no_sibling_or_forged_child`,
  `an_unattested_mcp_caller_sees_no_session_at_all`,
  `an_mcp_find_row_carries_no_mail_from_a_mailbox_shared_with_a_sibling`,
  `an_mcp_find_row_names_no_remote_parent`,
  `an_mcp_roster_shows_the_callers_subtree_and_probes_no_node`,
  `an_mcp_graph_names_no_project_path_node_url_or_far_cache_outside_the_scope`,
  `an_mcp_watch_refuses_a_sibling_run_by_id_tail_or_petname_as_unknown` and
  `an_mcp_watch_refuses_a_remote_target_as_unknown_and_never_fetches`.
- **A door that cannot approve never moves a parent edge and never launches
  a command of its own choosing.** The attested rules read the unsealed
  `parentSessionId`, so rewriting it IS approving. Every path that sets it is
  closed to `Mcp` and `A2a` at its head with `refuse_unapproving_door`
  (`graph/common.rs`: taught, `reason: door-cannot-approve`, audited):
  `reparent_refusal` (`graph/session_store.rs`) guards `session start
  --parent` and `graph link` when the id already has a record with another
  parent (a new id may register under a parent from any door, and the same
  parent again is a no-op). `session hook` (parent claim, rekey, stale-parent
  clearing) and `conduct` (it runs a command and upserts its record
  by id) refuse outright for those doors. `session_spawn_with` also refuses a
  `--id` naming a live record or an existing socket, so an attested caller
  cannot bind over another session's record. A new command that writes
  `parentSessionId`, or that launches a command, takes the same first line.
  Pinned by `an_attested_mcp_caller_cannot_adopt_a_session_by_reparenting_it`,
  `a_new_session_registers_under_a_parent_over_mcp`,
  `a_hook_from_a_door_that_cannot_approve_moves_no_parent`,
  `conduct_from_a_door_that_cannot_approve_launches_nothing`,
  `pending_deny_from_a_door_that_cannot_approve_is_refused_and_every_entry_stays`
  and `a_spawn_from_mcp_cannot_take_a_live_sessions_id`.
- **An MCP spawn or resurrect launches only as an attested caller, and a
  restore never does.** `session_spawn_with` judges the MCP door: the resolved
  session becomes the child's `--parent` (a `--parent` naming any other
  session is refused), and the `--prompt` is sent through `deliver_local_with`
  with that session as the sender, so the parent rule delivers it. With no
  sealed session the whole spawn is held with `hold_whole` under the shared
  `mcp` source, its id fixed so the approved launch registers under the one the
  caller was told, and `pending approve` replays it with the approver's door.
  An attested MCP spawn whose argv `wrapped_harness` does not recognise as a
  registered harness, or whose harness is named by a path (`/tmp/x/claude`,
  `./claude`, `env /tmp/x/claude`; `mcp_launch_is_bare`), is held whole the same
  way, shown as its full escaped argv (a shell, `aoide`/`strillix`, `env sh`, an
  interpreter, any unknown program). Only a bare harness name launches.
  It is never refused outright and never launched by the MCP call; only
  `pending approve` from a human door launches it. The accepted residual: a
  recognised harness is an agent with its own shell, so a launched MCP spawn can
  steer its own child the way Melete's `steer_run` does. `wrapped_harness` proves
  the walked name (`claude`, `kimi`, `pi`, `eidolon`), not the file behind it, so
  a file named `claude` anywhere passes; pinning a path is the open decision.
  `session_resurrect_with` is the same judgement for `resurrect`, because it
  too starts processes: attested, it revives; a door that cannot approve with
  no sealed caller (always the A2A door, which is never asked) holds
  `resurrect --project …` whole through the same `hold_whole`, as a
  `PendingSpawn` whose `path` is `resurrect`, and `pending_approve` dispatches
  on that path. A bare `resurrect` is refused there, not held (`reason:
  bare-resurrect-not-held`): it reads this process's working directory and
  launches the manifest's own commands, which a held line replayed from the
  approver's directory would neither show nor find. A path other than `spawn`
  or `resurrect` in a queue file is malformed and never launched
  (`PendingSpawn::is_known`). `resurrect` types its restore through
  `send_restore`, whose resolver names no sender: a restore rests on the door's
  `--yes`, so an MCP door parks it even when its server is attested. Pinned by
  `an_attested_mcp_spawn_launches_and_its_first_prompt_delivers`,
  `an_attested_mcp_spawn_cannot_name_another_sessions_child`,
  `an_unattested_mcp_spawn_parks_whole_and_nothing_launches_until_a_human_approves`,
  `a_full_mcp_bucket_refuses_one_more_spawn_and_launches_nothing`,
  `a_held_spawn_lists_as_its_whole_command_and_deny_launches_nothing`,
  `a_spawn_entry_that_does_not_parse_is_malformed_and_never_typed_as_a_send`,
  `an_unattested_resurrect_is_held_whole_and_launches_nothing_until_a_human_approves`,
  `an_attested_mcp_resurrect_launches_and_holds_nothing`,
  `a_bare_resurrect_from_an_unattested_door_is_refused_with_a_teaching_line`,
  `a_held_entry_names_only_a_command_an_approval_may_launch`,
  `an_attested_mcp_callers_restore_is_held_pending_though_its_send_would_deliver`
  and `a_restore_over_an_mcp_door_is_held_pending_and_never_typed`.
- **`pending.json` has one writer, and it holds the per-source cap and the
  per-entry size.** `record_pending` (`graph/send.rs`) drops the expired
  entries first (next invariant), then counts the entries carrying the new
  entry's `from` and appends under ONE stage lock, refusing
  at `PENDING_CAP_PER_SOURCE` without touching what is queued, and refusing an
  entry that serializes past `PENDING_ENTRY_MAX_BYTES` (a send's text and a
  spawn's argv alike) with `HoldRefused::TooLarge`, and refusing a write that
  would take the file past `PENDING_FILE_MAX_BYTES` (8 MiB, measured on the
  stored pretty form) with `HoldRefused::FileFull`. Every held send and
  `hold_pending` go through it, on every door. A new writer of the queue goes
  through it too, never around it, or one sender can fill a human's approval
  queue. A refused send carries `reason: PENDING_FULL` or `PENDING_TOO_LARGE`
  and is audited (an oversize one by the head of its text only). Pinned by
  `a_full_source_is_refused_and_audited_and_another_source_still_queues`,
  `an_entry_over_the_byte_cap_is_refused_audited_and_leaves_the_queue_alone`,
  `the_file_bound_refuses_a_hold_and_the_stored_file_stays_under_it` and
  `an_oversize_held_spawn_is_refused_audited_and_never_queued`.
- **A hold expires seven days after its `queuedAt`, and eviction runs before
  either cap.** `is_expired` (`graph/send.rs`) reads the stamp each time an
  entry is read, and expiry is never stored. An empty `queuedAt`, or one that
  does not parse, is expired, with no `expires`. `pending_list` reports
  `state` `pending`, `gone` (next invariant) or `expired`, and `expires`
  (`queuedAt` plus seven days). An expired entry's human line offers only
  deny. `pending_approve` refuses an expired entry before the take, taught, and
  launches nothing.
  `pending_deny` removes it. `record_pending` drops every expired entry under
  the stage lock before it counts a cap, and writes one
  `session.pending.evict` audit line per entry it drops, from whichever door
  queued the hold (`hold_pending` binds the A2A door, `hold_spawn` the MCP
  door) to the log the queuing call passes in (`audit_log_of`, or the A2A
  worker's own log). The line is written once the file that dropped the entry
  is on disk, so a failed write audits nothing; a failed append is not retried.
  A queue at its file bound refuses a new
  hold, taught, and takes it once an expired entry is evicted. Assume breach:
  a process running as the user can edit `pending.json` and age a hold past
  its week, mark a target's record `done` in `sessions.json` (its holds read
  `gone` at once), or delete the record (its holds read `gone` 4 h after they
  were queued). Approval stays on the person's terminal: the MCP door still
  cannot approve or deny, whatever the queue holds. Pinned by
  `a_hold_queued_over_seven_days_ago_lists_as_expired_and_offers_only_deny`,
  `a_hold_is_held_for_seven_days_from_its_queued_stamp_and_no_longer`,
  `approve_refuses_an_expired_hold_before_the_take_and_delivers_nothing`,
  `deny_removes_an_expired_hold_and_injects_nothing`,
  `an_empty_or_unreadable_queued_at_counts_as_expired_with_no_expiry`,
  `a_hold_evicts_expired_entries_before_the_source_cap_and_audits_each_one`,
  `a_held_spawn_evicts_expired_entries_and_audits_each_one`,
  `a_held_send_evicts_expired_entries_and_audits_each_one`,
  `a_hold_both_expired_and_gone_lists_and_refuses_as_expired`,
  `each_eviction_line_carries_the_door_of_the_call_that_triggered_it`,
  `a_failed_file_write_audits_nothing`,
  `an_eviction_from_a_spawn_held_over_mcp_goes_to_the_callers_log`,
  `an_eviction_from_a_send_held_over_a2a_goes_to_the_callers_log`,
  `a_queue_at_its_file_cap_refuses_a_hold_taught_while_nothing_in_it_is_expired`
  and `a_queue_at_its_file_cap_takes_a_hold_again_once_an_expired_entry_is_evicted`.
- **A hold is gone when its target can never take it, and approve refuses it.**
  `gone_reason` (`graph/pending.rs`) reads the session roster: a record whose
  `state` folds to `done` makes its hold gone at once, and no record at all
  makes it gone once `GONE_ABSENT_SECS` (four hours) have passed since
  `queuedAt`. A registered target that cannot be typed into yet is pending, not
  gone. A held spawn has no target and is never gone. Expiry is checked first,
  so an expired hold reads expired whatever its target. `pending_list` gives
  `state` `gone` and a deny-only line, and `pending_approve` refuses a gone
  entry before the take, taught with `reason: "gone"`, and launches nothing.
  Assume breach: the roster is a file a process running as the user can
  rewrite. A rewrite can make a hold read gone, which costs an approval and
  nothing more, since gone only ever refuses; it never redirects a delivery to
  another target. Pinned by
  `an_absent_target_stays_pending_until_four_hours_then_lists_as_gone`,
  `a_target_whose_record_is_done_lists_as_gone_and_approve_refuses_it_while_deny_removes_it`,
  `a_registered_target_that_cannot_be_typed_into_lists_as_pending_and_never_says_once_it_registers`,
  `a_gone_hold_that_is_not_expired_survives_an_eviction_pass`
  and `a_held_spawn_is_never_gone_even_when_its_id_names_a_done_record`.
- **An approver is shown what replays, in escapes, never stripped.** Every
  human view of a queued entry (`pending list`'s line, the answers of `approve`
  and `deny`, and across the crate boundary the conductor's PENDING rows) spells
  each character `is_unsafe` names, and any whitespace but a plain space, as
  an escape through `visible` (`graph/common.rs`): `\n`, `\x1b`, `\u{202e}`,
  a backslash doubled, and the combining marks and default-ignorable code
  points `visible` lists. A send's line is its whole escaped text, never
  clipped. A held launch's line is `visible_held`: every non-ASCII character is
  spelled too, so a lookalike letter reads as the escape it is. The JSON
  `text` is the bytes as they replay; `shown` is the one spelling every
  surface reads. A new view of an entry calls `visible` (or `visible_held` for
  a held launch) and does not clean, strip or clip the text itself. Pinned by
  `list_spells_unsafe_characters_in_every_field_a_human_reads`,
  `a_held_spawn_lists_as_its_whole_command_and_deny_launches_nothing` and
  `visible_spells_every_unsafe_character_and_never_drops_one`. A held spawn's
  `--id` is judged twice: at hold time (plain, at most 128 characters, naming
  nothing live) and again in `pending_approve` before the entry leaves the
  queue, since launching binds that id's socket; pinned by
  `approving_a_held_spawn_whose_id_has_gone_live_is_refused_and_it_stays_queued`.
- **Approval takes the entry it was shown, and re-checks what a hold checked.**
  `pending_approve` and `pending_deny` read the entry first, then `take_pending_entry`
  removes it under the stage lock only when its fingerprint is the `--expect`
  the caller named (the conductor sends the one its row drew) and it still
  passes `PendingSpawn::is_known`. `--expect` is a declared string flag on both
  verbs, so the CLI parser takes it, and both require it: a bare index is
  refused before the take with `reason: "expect-required"`, and the refusal
  prints the entry and the command with its fingerprint (`refuse_bare_index`).
  A stale value takes nothing; a blank one is refused with `reason:
  "expect-empty"`.
  A held resurrect's `shown` plan is checked
  by `resurrect::approved_selection` before the take, so a ledger that moved
  since the hold refuses without consuming the entry. Hold-time pins: a held
  spawn's `cwd` and a relative argv[0] are absolute, an `@file` is inlined
  when it is a regular file (a stat that follows symlinks, so `@/dev/stdin`
  and its kin are refused before any read), and `--instructions -` is refused
  (`reason: unpinnable-input`). The one unpinned
  input is a bare argv[0], resolved through the approver's `PATH`. Pinned by
  `deny_with_a_stale_fingerprint_takes_nothing_and_the_listed_one_takes_its_entry`,
  `approve_refuses_a_held_spawn_that_no_hold_would_queue_and_keeps_it`,
  `a_held_resurrect_refuses_to_launch_a_plan_its_approver_was_not_shown`,
  `approving_a_resurrect_that_revives_nothing_is_not_reported_as_launched`,
  `a_held_spawn_pins_its_cwd_and_relative_argv0_and_refuses_stdin`,
  `an_instructions_path_that_is_not_a_regular_file_is_refused_over_mcp_before_any_read`,
  `approve_refuses_an_empty_expect_before_the_take`,
  `deny_refuses_an_empty_expect_before_the_take`,
  `a_bare_approve_is_refused_and_prints_its_fingerprinted_command`,
  `the_printed_approve_command_approves_that_entry`,
  `an_old_approve_fingerprint_after_an_eviction_resolves_nothing`,
  `a_bare_deny_is_refused_and_prints_its_fingerprinted_command`,
  `the_printed_deny_command_removes_that_entry` and
  `an_old_deny_fingerprint_after_an_eviction_removes_nothing`.
- **Spawn ids are distinct per call and never a socket that answers.**
  A minted id carries a process-wide counter, since the clock is seconds
  (`spawn_id`). `spawn_id_in_use` is a `connect` probe, so a socket file nothing
  listens on does not hold an explicit `--id`. Over MCP an `--id` may name no
  existing record at all, live or done (`refuse_spawn_id`, `reason:
  id-in-use`), and a leading `-` is never a plain id. The child's argv binds
  the id as one `--id=<id>` token. Pinned by
  `two_holds_in_one_second_get_distinct_ids_and_both_queue`,
  `a_stale_socket_file_does_not_hold_an_explicit_id`,
  `an_mcp_spawn_refuses_the_id_of_a_done_sibling_and_keeps_its_edges` and
  `a_dash_led_id_is_a_bad_id_and_the_child_argv_binds_it_as_one_token`.
- **A parked send's source is derived by its door, never read off the caller.**
  `pending_origin` (`graph/send.rs`) is the one place an entry's `from` is
  chosen: the A2A door's own `--from` (`node:<name>` or `addr:<address>`, set
  by `aoide-server`'s `do_inject`, which no remote caller can reach), `mcp:<the
  caller's attested session>` else `mcp` on the MCP door, and the resolved
  attribution on the CLI and daemon doors. The cap counts that value and the
  approver is shown it, so a caller-chosen one would let a caller rotate its
  way past the cap, fill another source's bucket, or show an approver a sender
  it is not. What an MCP caller typed rides as `claimed`, cleaned
  (`common::clean_line`), shown under its own name and keying nothing. A
  `--from` spelling a door-derived form (`is_door_source`: `node:`, `addr:`,
  `mcp`, `mcp:`) on any door but the one that derives it is refused with
  `reason: RESERVED_FROM` where an entry is created; `pending approve` re-drives
  an entry with its recorded source and delivers, which parks nothing. A new
  door is decided in `pending_origin` as it is in `Door::can_approve`. The
  audit line of a send names the derived source as `from` and the MCP label as
  `claims` (`audit_send`), never the label as the sender. Pinned by
  `an_mcp_caller_cannot_name_a_node_as_its_source`,
  `rotating_from_over_mcp_does_not_escape_the_cap`,
  `an_mcp_callers_attested_session_has_a_bucket_of_its_own`,
  `a_cli_send_cannot_spell_a_source_only_a_door_derives` and
  `the_a2a_doors_own_source_is_recorded_as_given`.

## Invariants

- **Tracking a session gives it its mailbox.** The first insert of a record
  in `do_session_start_inner` (`upsert_session` reports it) calls
  `mail::box_add(id)`, a local act, best effort: a name the grammar refuses
  has no box and the start is not refused for it. A far letter to a name no
  local act created is still refused.
- **The hook door never moves a live record's pid and never trusts a payload
  pid it cannot vouch for.** `vouched_payload_pid` keeps a payload `pid` only
  if it is the hook process or one of its `/proc` ancestors (`hook_pid` is the
  door's own `SO_PEERCRED` stamp on the daemon arm); both the registration
  arm (`do_session_start` via `upsert_session`) and the existing-record
  refresh go through `aoide_storage::session::pid_may_move`. Anything that
  signals or injects by pid (`session kill`) re-verifies the seal against the
  live starttime (`terminate_with_key`); `conduct --id` refuses an id whose
  record is conductable or sealed (a conducted id is born once) and any id
  under the reserved `sub:` prefix; lineage-only readers (`eidolon::
  resolve_parent`, `resolve_parent_claim`) lean on the pid being immutable
  while the process lives, not on a seal.
- **The workspace default is stamped ONCE, through ONE seam, at THREE
  sites.** `SessionRecord.workspaceProject` (additive, `skip_serializing_if`)
  is written only by `graph/model.rs::observe_workspace` — the same seam
  `SessionRecord.workspace` is written through — and every site that observes
  a workspace calls it: `window.rs::attach_window` (the desktop's
  `session window attach --workspace`) and `reconcile_untracked_terminals`
  (both its upsert and its move branches) (the synthetic `win:` record a bare tty
  gets — a terminal on a bound workspace joins the project exactly as an
  agent's session does, so that publisher builds its record with
  `workspace: None` and lets the seam do the observation). A NEW adapter
  calls the seam; it never writes either field itself. The seam's rules are
  load-bearing: the default is stamped only when `workspace` goes from absent
  to present, only when the record has no explicit `project` and no default
  yet, and only when the workspace is BOUND — so a move never re-stamps,
  movement never clears it, an explicit choice is never stamped over, and
  binding a workspace adopts no session already sitting on it. An observed
  `None` DOES clear `workspace` (a window whose client reports no workspace
  has none) and never touches the default. The ladder itself —
  `project_for` = explicit > default > cwd, `effective_project_for` =
  explicit > owner > default > cwd — lives in the same file, with
  `workspace_default` the ONE reader of the stored name (a name that no
  longer resolves falls through; it is a default, not a choice).
- **Every grouping surface that can see a non-root session calls
  `graph::effective_project_for`, never bare `project_for`, for that
  session (ownership-graph lane, P-OWN S-A).** `project_for` stays the
  per-session primitive (own explicit project, else own cwd anchor) and is
  still correct on a ROOT session — no owner to walk, so the two agree by
  construction — but a caller that can hold a child (a `parentSessionId`
  chain already resolved, e.g. `who.rs`'s roster and `conductor`'s
  per-project count) must resolve through the owner chain or a child whose
  own cwd anchors nowhere silently falls out of its parent's group. A caller
  that only ever sees roots (`app.rs`'s `dag_rows`, `doc.rs`'s `anchors`
  edge and its Unicode render — each splits children off first) stays on
  `project_for`; switching it would be a no-op, never a bug, but it is also
  never a reason to add a second resolver. `effective_project_for` never
  writes a record and is never re-entered recursively by its own ancestor
  walk (each hop calls `project_for` on that one ancestor).

- **Those three root callers resolve the lead FIRST.** `doc.rs`'s edge
  build, `doc.rs`'s render and `app.rs`'s `dag_rows` each ask
  `leads_project(session)` (the project whose `Project.lead` names this
  session) before falling back to `project_for`/`effective_project_for`,
  then hand that index to `lead_over` — which yields the project's lead
  only when it is in the visible roster and is not the session itself.
  A session the lead covers hangs under the lead's node (`leads` edge,
  nested row) instead of anchoring; the lead itself is the one anchored to
  the project, whatever its cwd says. A lead missing from the roster
  resolves to `None`, so grouping is exactly what it was before a lead was
  named; never invent a parentless-session anchor for one. `leads_project`
  and `lead_over` are the only lead lookups — a fourth grouping surface
  reads them, never `Project.lead` directly.

- **A widget consumes the published `effectiveProject`, never re-derives it
  (P-OWN S-A2).** `doc.rs`'s session-node builder and `who.rs`'s
  `session_view_json` (shared by `node_json`/`group_json`) are the only two
  call sites that turn `effective_project_for` into JSON; both publish it
  additively — string, present only when the resolver resolves one, beside
  the STORED `project`, which stays untouched at both sites. No third
  publish site, no QML-side/client-side re-derivation of ownership from
  `parentSessionId` plus `project` — a consumer that needs the rendered
  group reads `effectiveProject` off the document it already has. Absent on
  every remote row (`sessions_from_graph` has no local session slice to
  walk an owner chain against) and on `node list` rows (that command's own
  concern) — never invented for either.

- **Codex titles use exact native thread IDs.** The reaper reads the configured
  Codex session index once per metadata pass and updates only already registered
  `agent == "codex"` records. The latest valid nonempty index title owns that
  field, including renames; every other field is preserved. Never enroll index
  history or infer a window, lifecycle event, or control channel from a title.
  Other harnesses retain their existing title precedence.
- **A Codex-app record is keyed by the native thread id, never a synthesized
  one.** `graph/codex_app.rs::reconcile_codex_app_threads` writes `sessionId`
  as the Codex thread id verbatim — the same id `session_index.jsonl`, the
  writer-lock filename, and the rollout's `session_meta.id` all agree on.
  No `codex:`/`app:`-style wrapper prefix, and no aoide-minted id: the
  native id is already the stable identity, and preserving it verbatim is
  what lets the reaper's `apply_codex_titles` name the card for free.

- **`kind:"app"` means "a task inside an app aoide does not conduct" and
  must stay closed to every agent-shaped signal.** It is why
  `reap::is_agent_kind` reads false for these records — keeping them out of
  BOTH `superseded_agent_duplicates` (N threads of one desktop app
  legitimately share one window address AND one pid; `superseded_by` reads a
  shared pid as the same process, so it would retire N−1 of them) and
  `is_session_dead`'s staleness arm (the record's `pid` is
  a SHARED app-server process, never proof any one thread is alive). Do not
  give an `"app"` record a registered agent profile, a hook, or a
  `conductable`/`socket` — there is no transport and no lifecycle event to
  back any of them.

- **An `"app"` record refuses `send`, `kill`, `session phase`, and `session
  end` by NAME and never falls back to another executor (P-CX-3, P-CX-5 S3b).**
  `graph/send.rs::deliver_local_with` checks `rec.kind.as_deref() ==
  Some("app")` BEFORE `is_conductable_now` and errors `codex-app-unsupported` —
  never today's `not-conductable`, which implies a retry might work.
  `graph/actions.rs::kill_target` checks the same field at the top of its walk,
  beside the `sub:` check, and refuses with `APP_OWNS_PROCESS` — the app owns
  the process, never a signal this crate sends.
  `graph/session_store.rs::do_session_phase_inner`/`do_session_end_inner` check
  the same field (`refuse_app_record`) before touching either stage file and
  refuse with the same `codex-app-unsupported` reason `send` uses —
  `apply_codex_capture` is the one writer for an app record's `state` from here
  on, and neither of these commands is it. `--to` folds into
  `deliver_local_with` before ever reaching a socket, and A2A inject
  (`aoide-server`'s `a2a::do_inject`) reaches that same gate, which parks it, so
  that's the ONE gate for every send path; don't add a second `kind:"app"` check
  anywhere else.

- **No path may synthesize a per-thread window address.** A Codex-app
  record's `windowAddress` is the window owning the process that holds that
  thread's writer lock, stamped ONLY by the desktop's window sweep, through
  `session window attach` — core resolves no window. `reconcile_codex_app_threads` itself never touches
  `windowAddress`/`workspace`; N threads of one app therefore share one
  address by construction, and focusing any of them raises the app, which
  is the only thing the app lets anyone do.

- **Codex-app liveness is a try-flock, never `/proc`.** `lock_is_held`
  (`codex_app.rs`) answers "is this thread live" by attempting
  `LOCK_EX|LOCK_NB` on the thread's writer-lock file and reading
  `EWOULDBLOCK` as held; the file is 0 bytes and carries no pid, and a
  missing file is opened `O_RDONLY` only, never created. No code path may
  substitute a `/proc` read, a pid file, or a heuristic for this signal.

- **Discovery is the one parsed `ps` table; Linux gets no separate path.**
  `codex_app_servers` keys ownership on the app-server argv (`argv0` basename
  `codex` plus a bare `app-server` token) against a single
  `ps -axo pid=,ppid=,command=` table shared by every OS. The only
  `cfg(target_os = "linux")` branch this design permits is
  `holder_via_proc_fd`: an app-server owns a lock only when its own
  `/proc/<pid>/fd` table holds it — the sole evidence, for one server
  or many, never a second discovery implementation, and never
  consulted for liveness.

- **A desktop thread enrols only on positive ownership evidence.**
  An app-server owns a lock only when its own fd table holds it
  (`holder_via_proc_fd`, Linux); a held lock with no such owner is a
  CLI thread or unknown and gets NO app record. A tracked non-app
  record already sitting on that id is never touched either way
  (root ruling, P-CX-2b).

- **A failed or incomplete desktop-Codex scan is `Unknown`, and `Unknown`
  changes no record — only a positively observed set does (Codex ruling
  seq 211, P-CX-4).** `codex_app.rs::ThreadScan` is the seam: `Observed`
  carries positive evidence for every lock in the directory, including a
  positively observed empty set (a genuine close still disappears);
  `Unknown` means some step of the gather — the lock directory, a lock
  file, the process table, or an already-enrolled thread's fd table —
  could not be read, and says nothing about any thread.
  `reconcile_codex_app_threads` returns `(sessions, false)` untouched on
  `Unknown`: nothing removed, nothing inserted, nothing upserted, no
  petname re-minted. `sync_codex_app_threads` takes no stage lock and
  writes nothing on `Unknown` either. A failed observation is never a
  confirmed exit: a discovery step that swallows its own error into an
  empty `Vec` turns every enrolled thread into a drop-and-re-mint loop the
  moment one writer lacks what the scan needs (the log holds the incident),
  so every dead end in the gather surfaces as `Unknown`, never as a silent
  `Observed(empty)`.

- **A Codex capture never enrols, removes, or revives a thread, and a field
  it reports is `None`/unchanged unless its own record was read WHOLE
  (P-CX-5, codex seq 228 rulings).** `graph/codex_capture.rs::capture_for`
  is a bounded read (`TAIL_BYTES` off the END of a rollout); a missing
  rollout, an unreadable one, or a tail whose only content is one record too
  large to ever land whole in the window all yield `CodexCapture::default()`
  — never inferred as idle/completion, never a reason for
  `codex_app.rs::sync_codex_app_threads` to touch a record's roster
  membership (that's `ThreadScan`/`reconcile_codex_app_threads`'s call
  alone). A tail cut that opens mid-record drops that leading fragment
  WHOLE, never parsed — the same rule an encrypted `reasoning` record and a
  truncated `task_started` already hold in the fold itself. The merge onto a
  `kind:"app"` record (`apply_codex_capture`) touches `state`/`say`/`tool`/
  `activity`/`model`/`context_tokens`/`context_ceiling`/`sources`, each set
  only when the capture produced a value and only when it differs — never
  blanked back out by a quiet tick. `state` is folded to `working`/`idle`
  off the rollout's own open-turn bracket — the fold's own vocabulary has
  no `awaiting` to produce (P-CX-5 S3, every consumer of an app record's
  `state` enumerated by file:line under ruling R2 first) — and no OTHER
  writer can land one there either: `session phase`/`session end`
  (`session_store.rs::do_session_phase_inner`/`do_session_end_inner`)
  refuse a `kind:"app"` record outright with `codex-app-unsupported`, the
  same reason `send`'s own refusal already uses (P-CX-5 S3b, closing the
  gap S3's removal of the per-tick idle reset opened: nothing else would
  have healed a stray write). A `None` read (unreadable, missing, or
  momentarily empty rollout) leaves the record's last-known `state` alone
  rather than resetting it. `title` joined that merge at P-CX-5 S4,
  FILL-ONCE: a captured `nickname` lands on `rec.title` only while the
  record's own title is empty, never overwriting one already set.
  `parentSessionId` is S4's other half but never `apply_codex_capture`'s to
  set — `codex_app.rs::apply_codex_lineage` is its one writer, and holds
  its own invariant below (R3's "one producer per shape" still means
  `sources` never grows a second writer without its own function).
  `capture_for` memoises per thread (a process-static, no
  signature change) so a live thread's ~1 Hz callers don't each re-scan a
  rollout's whole prefix just to count newlines: an unchanged `(len, mtime)`
  returns the prior capture with no I/O, and append-only growth reuses the
  memo's own tail-alignment facts unchanged as long as the window stays
  under a small bounded slack past `TAIL_BYTES` — never an unbounded read. A
  rollout that shrank or was rewritten in place is never trusted half-way;
  its memo entry is dropped and the read starts over from scratch. The
  memo is keyed by thread id, one `aoided` per desktop-Codex home so this
  never collides, and `sync_codex_app_threads` calls `retain_capture_memo`
  once per tick with that tick's own observed thread ids so a thread that
  stops being live has its entry evicted rather than held forever.

- **A native `parent_thread_id` becomes `parentSessionId` only when the
  parent exists locally, isn't the record itself, and wouldn't cycle — and
  it grants NO authority (P-CX-5 S4).** `codex_app.rs::apply_codex_lineage`
  is the one writer of a `kind:"app"` record's `parent_session_id`; it sets
  the edge only when ALL of: `cap.parent_thread_id` is `Some(pt)`; `pt` is
  not the record's own id (no self-loop); a record with `session_id == pt`
  EXISTS in the sessions roster, regardless of that record's own `state`
  string — a FUNCTION-level rule, not a modeled process-exit path, since
  `reconcile_codex_app_threads` already drops a dead app thread's record
  before this function ever runs against the same tick's roster; a native
  id naming no local record at all is dropped as unresolvable provenance,
  never written as a promise; and the edge would not close a cycle
  (`doc.rs::would_cycle`, the same guard `manage.rs::link` and
  `session_store.rs`'s reparent already use — no third cycle-detection
  shape in this crate). It is idempotent: a resumed thread's `session_meta`
  re-read on every capture reapplies the identical edge as a no-op, never
  re-setting or duplicating it. The edge changes nothing else about the
  record — `rec.kind`/`rec.agent` stay exactly what
  `reconcile_codex_app_threads` re-applies every upsert, so
  `actions.rs::kill_target`'s `APP_OWNS_PROCESS` and
  `send.rs::deliver_local_with`'s `codex-app-unsupported` refusals still
  fire on the very first `kind:"app"` hop, before either ever reaches a
  parent walk — an ancestry edge is not a transport or a kill grant.

- **Lineage candidates are checked one edge at a time against the LIVE
  roster, never a snapshot frozen before the tick's own edges land
  (root order seq 404 fix).** `codex_app.rs::apply_codex_merges` is
  `sync_codex_app_threads`'s one caller of both `apply_codex_capture` and
  `apply_codex_lineage`: every `"app"` record's capture is applied first
  (no cross-record read needed there), then every candidate parent edge is
  collected and applied IN ORDER, each one judged against `sessions` as it
  stands at that moment — including any edge an earlier candidate in the
  SAME pass already wrote. A prior implementation judged every candidate
  against ONE snapshot taken before the pass started; that let two app
  records captured in the same tick whose own `session_meta` each named the
  OTHER as parent both look acyclic against the same starting picture and
  both land, a two-node cycle. The live-roster check closes that: the
  second candidate checked finds the first's edge already in place and its
  own candidate refuses via the ordinary cycle check — no new rule, the
  same per-record gates applied against a moving target instead of a frozen
  one.

- **`thread_source` publishes as `SessionRecord.native_role`
  (`nativeRole` on the wire), never a `kind` reclassification (root order
  seq 404).** `codex_app.rs::apply_codex_capture` sets it from
  `cap.thread_source` verbatim, ALWAYS-SET like `say`/`tool`/`model` (never
  fill-once like `title`'s `nickname`), through the same
  `MERGED_SOURCE_FIELDS` table those fields use (`thread_source` →
  `nativeRole`) — so its `sources` pointer rides the same way, and a
  capture read failure never clears a previously published value. A
  desktop-Codex `kind:"app"` record stays `"app"` regardless of what
  `native_role` says: today's tree-nesting rendering keys off `kind`, so
  `parentSessionId` alone does not nest an app child under its root
  (root order seq 404) — publishing `nativeRole` beside `parentSessionId`
  is what lets such a consumer recognize the relationship explicitly,
  rather than this crate ever reclassifying `kind` to `"subagent"` for
  presentation.

- **An eidolon presence record is keyed by its native id verbatim, exactly
  like a Codex thread (P-EIDOLON, slice E1b).** `graph/eidolon.rs::
  reconcile_eidolon_sessions` writes `sessionId` as `meta.json`'s own `id`
  unchanged — no `eidolon:`-style wrapper prefix. Unlike a Codex-app record
  this one carries `kind:"agent"`, not `"app"`: an eidolon session IS a
  conducted-adjacent agent session, a normal dedup/staleness candidate once
  enrolled.

- **Eidolon liveness is the presence socket's answer to `{"op":"ping"}`,
  never a stat, never `/proc`.** `presence_probe` (`eidolon.rs`) takes the
  scan's one deadline (its start plus `PRESENCE_DEADLINE`, 250ms) and bounds
  every step by what is left of it: the connect through
  `aoide_protocol::local_socket::connect_bounded`, then the write and each
  read. The scan probes every directory at once, one scoped thread each, and
  reads a `meta.json` only where a socket answered or was held aside. The
  outcome sorts three ways. `Answered`: exactly `{"ok":true}`, live.
  `NothingListens`: no socket file, or a connect that failed other than by
  timing out; not live, never a scan failure, and its `meta.json` is never
  read. `Silent`: held, but no reply by the deadline, a connect that timed
  out on a full backlog, a reply over 4096 bytes, or any other reply. A
  silent directory is held aside and is live only while its `meta.json`'s
  `pid` is a row of the process table, as Eidolon's own sweep pairs silence
  with the registration's pid. Residual: if another process reuses the pid
  of a session whose socket is silent, that session stays live until that
  process exits, so the roster shows it and the resume gate skips its
  journal.

- **Aoide never sweeps or writes under the eidolon presence root.**
  `$XDG_RUNTIME_DIR/eidolon` is read-only territory: this crate lists it and
  reads `meta.json`/probes `sock`, and nothing here ever removes a stale
  directory or writes a file under it. A presence entry whose socket has gone
  quiet loses its Aoide-side record the next `Observed` pass finds it missing
  (below); the directory itself stays the producer's own to clean up.
  **The ONE exec of the `eidolon` binary is the producer's own read-only
  export** — `eidolon log --json <journal> [--after <id>]`, the door
  `docs/architecture/EIDOLON-TRACE.md` states — and it is reached only through
  the harness CAPABILITY (`TranscriptSpec::locate` + `TranscriptSpec::trace` in
  `aoide-protocol`), only when the producer's own `log --help` proves it is the
  read-only generation, and never against a journal whose producer generation
  cannot be proven. Nothing else in this crate runs that binary, and nothing
  here ever asks it to write.

- **TUI-vs-not is read off the SAME parsed `ps` table `codex_app.rs` already
  owns — no second discovery path.** `eidolon.rs::is_tui_argv` classifies a
  presence pid's argv (no subcommand, or an explicit `tui` token, is the
  TUI) against a row of `codex_app::process_table`/`parse_process_table`'s
  output; a presence pid absent from that table degrades to `tui: false`
  (never a guessed `working`/`idle`), never a second `ps` invocation or a
  raw `/proc/<pid>/cmdline` read.

- **State is folded from `busy`+TUI-ness by the SAME function that
  reconciles the roster (E2 folded into E1b) — a TUI owner's `busy` is a
  real fact, anything else is the literal `"unknown"`.** A TUI owner's
  `busy:true`/`false` maps to `working`/`idle`; eidolon's own producer never
  sets `busy` outside the TUI (a permanent defect on its side, not a gap
  here), so a non-TUI owner's record carries `state:"unknown"` rather than
  a guessed verdict — `aoide_protocol::canonical_state("unknown") ==
  "idle"` is the vocabulary's own fold for that, pinned by its own test, and
  is NOT a bug to route around. `awaiting`/`error`/`cancel` are never
  produced by this function: eidolon's pending-approval and streaming
  events live only on its own in-process event bus, never in `meta.json`.

- **`parentSessionId` is resolved here, unlike a Codex-app record.** The
  first ancestor up the presence pid's own `/proc` chain
  (`aoide_storage::attest::pid_ancestry` in production, injected as a pure
  closure for tests) that is itself `conductable == Some(true)` and not
  `done` becomes the parent; no such ancestor leaves the record top-level.
  The wrap's own record — and its petname — is only ever READ by this walk,
  never written.

- **A petname mints only on insert, never on an upsert of an
  already-enrolled record.** `reconcile_eidolon_sessions` calls
  `aoide_storage::petname::mint_for` exactly once per NEW presence id, the
  same insert-only timing `reconcile_codex_app_threads` holds; a rescan of
  an unchanged presence must never re-mint one, and `windowAddress`/
  `workspace` are left empty here for the desktop's window sweep to fill — this
  module reads no compositor either.

- **A record whose presence stops being observed is dropped only on an
  `Observed` pass, never on `Unknown` — identical to the Codex ruling
  (P-CX-4) applied to this module.** `eidolon.rs::PresenceScan` is the seam:
  `Observed` carries positive evidence for every directory under the
  presence root, including a positively observed empty set (a missing root
  is `Observed(empty)`, not `Unknown` — no eidolon session has ever
  registered, a genuine fact); `Unknown` means the root could not be listed,
  a `meta.json` could not be read/parsed for a reason other than simply
  having vanished since the listing, or the process table came back absent
  once a socket had answered or been held aside. `reconcile_eidolon_sessions`
  returns `(sessions, false)` untouched on `Unknown`, and
  `sync_eidolon_sessions` takes no stage lock and writes nothing either.

- **The reaper dispatches an eidolon record's profile off `rec.agent` alone,
  never an env var.** `reap.rs::profile_for` already resolves any record
  generically via `aoide_protocol::agents::agent_profile(&rec.agent)`; an
  `agent:"eidolon"` record enrolled by `sync_eidolon_sessions` reaches
  `EIDOLON_PROFILE` through that same one path, with no eidolon-specific
  branch added to `profile_for` itself.

- **The LAST trace record decides an eidolon record's state — never
  inferred from the presence `meta.json` while a trace exists
  (P-EIDOLON E6, `docs/architecture/EIDOLON-TRACE.md`).**
  `eidolon.rs::eidolon_state(busy, tui, trace)` consults the trace FIRST and
  returns its fold outright when `trace` is `Some` (possibly EMPTY): the
  presence's `busy`/TUI-ness never overrules it, and never fills in for a
  trace that is empty or whose last record Aoide cannot read —
  `eidolon_state_from_trace` returns `None` there and the function carries
  the literal `"unknown"` (the vocabulary's own absence-of-evidence arm)
  rather than a guess from a weaker signal. `read_presence_trace` is the one
  place a presence becomes lines: it resolves the trace through the harness
  CAPABILITY — `TranscriptSpec::locate` (the same locator the reaper's
  transcript refresh and `session trace` use, which answers a current mirror,
  else the journal when the producer's read-only export door is there, else the
  presence file) and then `TranscriptSpec::trace` (the one trace reader) — so a
  producer that publishes no mirror still feeds this state fold. A trace that
  does not resolve at all, or that is not a trace, is `None` — the older-
  eidolon shape, which is exactly when the presence rule below applies. Don't
  reorder the two rules, and don't soften the `None`/`Some(empty)`
  distinction: it is what makes a headless run (whose `busy` is permanently
  false, eidolon's own producer-side defect) finally carry a real state.
- **`session trace` is read-only and sits outside every session-WRITE
  discipline.** `graph/trace.rs::session_trace` takes no stage lock, never
  calls `aoide_client::daemon::daemon_dispatch` (the L4 dual-writer prefix
  every `session start/phase/end`, `session hook`, `session reap` and
  `session grant exempt` carries), and writes nothing — it loads
  `sessions.json`, reads a trace file and prints. Don't fold it into that
  family "for consistency": there is nothing to route to the daemon.
  `<id>` resolves through `aoide_storage::addr::resolve`, the SAME resolver
  `send --to` and bare `session`'s filter use (never a second one, and no
  hub preference — that is a routing rule for `send`, not a rule for reading
  a file that lives on this box, so a `node/<rest>` target is a taught
  refusal rather than a remote read). The trace is reached through the
  harness CAPABILITY `TranscriptSpec::trace`, never by naming a harness by
  string, and the path through `TranscriptSpec::locate` — the same locator
  the reaper's transcript refresh calls. `--follow` is CLI-only (a follow
  that parks a connection makes no sense over MCP/A2A/the daemon socket),
  with the `events tail`/`secrets watch` signal-loop shape.
- **An ended session is named by its exact ledger `sessionId`, and by
  nothing else.** When `addr::resolve` finds no roster row, `session_trace`
  reads the newest ledger line whose `sessionId` equals the target
  (`ledger_ended`). A petname or a prefix of an id names no ended session, so
  the match stays exact. The line's `agent`, `cwd` and `logPath` stand in for
  the roster row, the message says the session ended and when, and `--follow`
  prints the window and returns: no presence is left to follow. A `done`
  record returns the same way: a session that has finished has nothing new to
  follow.
- **Trace text is stripped on the way out.** Every text `session_trace` prints
  or puts in its data passes `aoide_protocol::untrusted::strip_unsafe`, under
  the trace's own clips: `one_line_clipped` counts what is shown, and
  `clip_keep_ws` makes a tab one space first. `--follow --json` lines are
  stripped too. A new text site the trace prints gets the same strip.
- **A record's `logPath` is the trace's hint only outside
  `session_logs_dir()`.** It goes to `TranscriptSpec::locate` as the hint,
  the way `dropped_this_pass` passes it. A path inside `session_logs_dir()` is
  a terminal's transcript, never a journal, so it is never passed. Without
  that filter a wrapper record whose presence is gone would read a transcript
  as a journal.
- **`data.steps` is PROJECTED from the same window `data.lines` carries, and
  bounded before it is built (Phase B).** `steps_of` is the one projector: one
  step per non-empty content block in the record's own order, and `render_line`
  renders `steps_of(record, Clip::Line)` so the human line and the JSON cannot
  drift. Never "one step per record" (`Option<Step>`) — an `AssistantMessage`
  routinely holds thinking + text + tool_use, and a single-step projector drops
  all but one silently. Never remove a bound to make a step list longer: the
  `--tail` window, `MAX_BLOCKS_PER_RECORD`, `MAX_STEPS` and `Clip` are what keep
  one hostile record (or one hostile journal) from making an answer unbounded,
  and every cut must stay NAMED (`Step::clipped`, the `+N more block(s)` note,
  `stepsOmitted`) — a silently shortened list is the bug these exist to
  prevent. `Clip::Detail` widens ONE clip (blocks' own line breaks, tool
  arguments); it never turns the projection into the journal.
- **The bridge's trace query is bounded by the DOOR, not by the caller's
  cadence.** A polled read (`sessiontrace`) must answer or refuse on the
  connection; never leave a parked caller. `parse_command` refuses a blank
  `sessionId`, a nonsense `lines` and an unknown `clip` (clamping `lines` from
  above only), and `handle_conn` still ANSWERS a line naming that verb which
  fails its gate (`bad-request`) — silence is not an answer. `run_bin_bounded`
  (`run_core_bounded` is its core-binary wrapper; the menu's read calls it
  with `strillix`) is the only way this module runs a READ's child: a wall-clock
  deadline with kill + `wait` (no zombie, no live child past the bound), pipes
  drained by SLOT-COUNTED reader threads that are NEVER joined (`ReaderSlot`; a
  descendant that inherited a pipe must cost a slot, never the deadline), and a
  partial read DISCARDED rather than parsed when EOF does not arrive inside the
  grace. Don't reintroduce a `join()` on a reader, and don't add a bound that
  one timed-out tick can outrun. Refusals are audited; a successful poll is not
  (one line per second per card is not a human gesture), and an audit line never
  carries an argument value.

- **`session bind` assigns continuity, never authority.** Keep the operation
  daemon-owned and local-only; no missing-daemon fallback. It does not load
  optional Mneme config, change grants, or replace executor-specific mail
  reader keys. A binding is immutable within a session; same-key calls are
  no-ops. Project the explicit binding into the graph and exit ledger.

- **This crate is core, never strillix.** Nothing here may gain a
  wayland/image/quickshell dependency — that's exactly what P-A1 moved OUT
  (to `screen`) to keep this crate headless-safe. `cargo tree -p
  aoide-conduct` staying free of those deps is a standing gate.
- **`shellbridge.rs`/`notifications.rs` are a named, deliberate charter smudge.**
  Their CLI commands live in `strillix`; the files stay here because `permit.rs`
  (this crate) publishes through `notifications`, and `conductor/ui.rs` reads the
  socket path `shellbridge` owns. Don't move the files to chase the commands —
  see `docs/architecture/PACKAGE-LAYOUT.md`'s "Charter exceptions" for the
  full reasoning before touching either.
- **The socket's verb table is a registration seam, not a second parser.**
  `run(verbs)` takes the binary's `&'static [Verb]`. A line that no core parser arm
  claims goes to the first entry naming its `cmd`. With no entry, `drop_unparsed`
  audits it and drops it, except that a line naming `sessiontrace` is answered.
  Core registers nothing. A registered verb whose caller is parked on a reply calls
  `drop_unparsed` itself for a line it cannot parse, so every drop is audited the
  same way. The desktop verbs are strillix's (`pkgs/aoide/crates/strillix/AGENTS.md`);
  don't re-add them here.

- **The bridge's action whitelists never become a translator.** It is no
  longer just five session actions: `session_action_args` (subject: a
  session id) and `project_action_args` (subject: a project name, no
  session id anywhere — not on the wire, in its plan, its reply, or its
  audit line) are each the ONE authority for their own closed whitelist and
  the ONE call site `parse_command`'s wire gate consults for it; nothing
  else builds argv from wire values it did not validate. Both plans run
  through the same subject-agnostic sequencer (`ActionSubject`,
  `dispatch_session_action`/`run_session_step`/`session_action_reply`) so
  the two whitelists share one runner instead of two copies of it — the
  five original session actions are byte-identical through that sequencer
  to before the split. The third closed whitelist, `workspaceaction`, lives in strillix's bridge
  (`pkgs/aoide/crates/strillix/AGENTS.md`), and it shares `run_core_step` and
  `outcome_triple` with the two above, so every CLI answer is read in one place. `safe_session_id` (session ids: no whitespace) and
  `safe_action_value` (project/host/create/edit names: ordinary spaces
  allowed) are two different checks — an id is a bookkeeping key, a name is
  user-facing text — and both reject empty, `-`-prefixed, or
  control-charactered strings before they reach an argv; `safe_action_path`
  is the rule for path arguments — absolute and control-free, whitespace
  included — with no count cap: the wire line's own length is the only
  bound. A host name in `projectaction`'s `hosts` array is checked with
  `valid_node_name` alone — pure, no `load_nodes()` call at the bridge
  layer, since the CLI is the one authority for whether a host is actually
  registered. The `project` field must be a JSON string: `""` is the clear
  request, and a missing key, `null`, or a non-string value is refused
  outright, never read as a clear. The reply forwards the CLI outcome's
  `data` verbatim under `"data"` when the envelope has one and omits the
  key otherwise — never reshape it. The reply is one JSON line on its own
  dedicated connection and the QML callback contract is exactly-once. A
  multi-step action (`createproject`'s add-then-assign, or a
  `projectaction` with hosts) stops at its first failure and reports the
  partial state honestly rather than rolling back. The bridge never
  pre-checks what the CLI already refuses — `project add --new` owns "this
  name exists," `--host` owns "this host is registered," neither is this
  file's job. No reply channel on the connection means no dispatch at all.
  The audit line carries only the action name and status, never an
  argument value. The accept loop gives every connection its own thread,
  with no read timeout added: a client is allowed to idle, and one that
  does must never starve another's.
- **`normalize_addr` is `pub`, not `pub(crate)`, on purpose** — `screen`
  reaches it directly rather than duplicating it. Don't narrow it back
  without checking that dependency first.
- **`commands::hooks::skill_source` is `pub`, not `pub(crate)`, on purpose**
  (P-I2, ONBOARD.md decision 10) — `aoide-cli`'s `onboard` reaches it
  directly for its from-a-checkout refusal (the only repo-root detector in
  the tree) rather than re-deriving the walk-up. Don't narrow it back
  without checking that dependency first.
- **`reap::proc_exists` is `pub` on purpose** (task #33) and is a RE-EXPORT of
  `aoide_storage::fs::pid_is_alive`, core's shared liveness probe (POSIX
  `kill(pid, 0)`; `ESRCH` alone absent, every other errno live).
  `aoide-server`'s A2A `tasks/get` consumes it: don't narrow it back, and don't
  turn it into a local `/proc` read.
- **A killed terminal never self-reports `done`.** `reap` is the only
  sanctioned sweep of dead sessions; don't add a second liveness mechanism.
  Reaping now also runs IN the daemon's own tick (P-D6, ~12s cadence) when
  one is resident — that is the SAME sweep (`reap`), not a second one; the
  ~12s systemd timer's own `session reap` becomes a redundant backstop, never
  a third mechanism.
- **`reap` collects two kinds of leavings a killed session left in
  `$XDG_RUNTIME_DIR/aoide`, not one.** `sweep_orphan_sockets` and the
  ssh-transport lane's tunnel sweep are the SAME sweep pass conceptually:
  only a roster-less, settled candidate is ever touched, and a candidate
  whose session IS still live is spared outright regardless of what a
  pid/port probe would say. **"Roster-less" is NOT the same test for the
  two sweeps, on purpose.** The socket sweep treats any session record
  still present in `sessions.json` as live, whatever its `state`. The
  tunnel sweep is narrower: a session already `done` — a clean exit that
  ran `close_all_for_session` but has not yet been pruned off the roster
  (`prune_done` only fires on a pass that reaped something) — counts as
  gone for tunnel candidacy specifically, so a record `close` had to KEEP
  (its child survived the fast path's own bounded kill) does not wait,
  unbounded, on `prune_done`'s own schedule. This asymmetry is deliberate
  and narrow: it changes ONLY `orphan_tunnel_candidates`' own roster
  computation in `reap_inner`, never `sweep_orphan_sockets`'s, and never
  `prune_done` itself — a `done` session's control socket is already gone
  by the time `do_session_end` returns, so the socket sweep has no
  equivalent problem to solve. **`kill_if_still_our_ssh`'s bool return is
  `#[must_use]` and gates removal at every call site, never discarded —
  `close`, `open_or_reuse_with`'s stale-record path, and
  `sweep_orphan_tunnels` all unlink/replace a record ONLY on `true`
  (confirmed dead or never actually ours); on `false` (confirmed ours and
  still alive) the record is left exactly where it was, so the reaper's
  next sweep pass re-gathers the SAME record and retries the kill — no
  separate retry bookkeeping, and no candidate is ever unlinked out from
  under a live, untracked child.** **The two sweeps are NOT symmetric about
  the stage lock, on purpose.** `sweep_orphan_sockets` runs entirely inside
  `with_stage_lock` (`reap_inner`) because unlinking a leftover socket file
  is cheap. A tunnel candidate's `ssh -N` child may still be alive, and
  signaling it (`aoide_client::tunnel::kill_if_still_our_ssh` →
  `terminate_pid`'s bounded `SIGTERM` + `waitpid`/`/proc` poll) can take up
  to ~1s PER candidate — doing that under `.stage.lock` would serialize
  every other stage writer (hooks, the ~1Hz conduct ticks, the window
  listener, `session start`/`end`, `send`) for as long as it takes. So the
  tunnel sweep is split: `reap_inner` only GATHERS candidates under the lock
  (`orphan_tunnel_candidates` — roster/settle checks against already-loaded
  state, no process signaling), and `reap` runs the KILL half
  (`sweep_orphan_tunnels`, never re-implemented here; `aoide-conduct` sits
  above `aoide-client` in the crate DAG and reuses `kill_if_still_our_ssh`
  verbatim) AFTER `with_stage_lock` returns, folding the result into the
  same `Outcome` the lock-held pass already built (the `gathered_addrs`/
  `window_owners` split `reap` already holds for its own pre-lock window
  gather is the same shape, mirrored on the other side of the lock instead).
  **Any new candidate-collector that must probe or signal a live process
  belongs on the post-lock side of this same split — never inside
  `reap_inner` "for consistency" with the socket sweep.** Nothing about a
  tunnel may become a resident daemon either way:
  `session_store::do_session_end` closes a session's own tunnels on its
  clean exit (`aoide_client::tunnel::close_all_for_session`, the fast path,
  likewise run AFTER the stage lock releases since it may block on a real
  `waitpid`); the reaper's tunnel sweep is only the backstop for the session
  that never got to run that exit path, or the retry for one whose
  fast-path kill didn't finish in time. A new leaving-kind under this same
  runtime directory joins the sweep the same way — never a separate cleanup
  mechanism.
- **The roster exit drops letters after the roster write, never under it.**
  `drop_ended_sessions` (`graph/session_store.rs`) is the one helper the three
  exit paths (`do_session_end_inner`, `record_hook_deaths`, `reap_inner`) call
  with the ids they ended, once their roster write has returned.
  `sweep_session_letters` reads the bound sessions from the outbox before it
  reads the roster, so no path holds both locks at once, and an unreadable
  roster skips the session pass rather than reading as an empty one. The
  pre-131 pass that runs first takes every receipt, whatever its session.
- **Every session-write handler routes through `aoide_client::daemon::
  daemon_dispatch(inv)` FIRST, as its own first line, falling back to its
  pre-existing direct stage-write path byte-identically on `None` (P-D6,
  `docs/architecture/AOIDED.md`'s "L4").** `session start/phase/end`,
  `session hook`, `session grant exempt` and `reap::reap_and_announce` all
  follow this exact one-line prefix. Only the CLI door routes:
  `daemon_dispatch` answers `None` for any other door, so an MCP or A2A
  write runs the direct path under its own door and never reaches the
  daemon as `Door::Daemon` (whose Stop hook rings). Never add a door check
  at the call site; the hop owns it. A new session-write handler joins the
  family the same way — see `client`'s own `AGENTS.md` extension-point
  note. **`graph/
  undying.rs`'s `undying_grant`, `graph/spawn.rs`'s `--undying` mark, and
  `graph/resurrect.rs`'s undying transfer are a deliberate exception, not an
  oversight:** `state/undying.json` is not a `state/stage/` file, so none of
  the three has L4 residency to route through — don't add a `daemon_dispatch`
  prefix to any of them "for consistency" with the family above; that would
  put an extra writer on a file the undying store's own atomic-write CRUD
  already lets multiple call sites share safely (each call is load-mutate-
  save on the whole set, never a partial write).
- **The check lane's three hook calls are best-effort and gated on the EXACT
  event, never on a broader phase family (task #139).**
  `graph/send.rs::hook_for_profile` calls `aoide_upkeep::checklane::
  on_session_start` only from `HookAction::Start`, `checklane::
  on_prompt_submit` only inside `HookAction::Phase` guarded on
  `phase == "working"`, and `checklane::on_stop` only inside the same arm
  guarded on `phase == "stopped"` — the one phase string each of
  `UserPromptSubmit`/`HookClass::Stop` alone produces (`map_hook`); don't
  widen either guard to fire on `awaiting` too, or the lane's own delta
  arithmetic (baseline vs. "what changed") stops meaning what its message
  claims. `on_session_start`/`on_stop` take the raw payload's OWN `cwd`
  (present on every real hook payload, not a stage-file lookup);
  `on_prompt_submit` takes neither `cwd` nor config — it only drains a file.
  All three degrade to a no-op on a missing `cwd`, a disabled lane, or an
  unloadable config — same best-effort stance every other hook action in
  this function already holds; a check-lane failure must never fail the
  hook. **The `HookAction::Start` arm reads `session_store::stored_phase(id)`
  BEFORE its own mutating calls** (`do_session_start`, then
  `do_session_phase_if`) to derive `mid_turn` — reading it after either
  would risk observing a phase this same event already changed. Never
  substitute the hook payload's own `source` field for this: a manual
  between-turns `/compact` says `source: "compact"` too, but IS a settled
  boundary — only the stored phase tells the two apart.
  **`commands/hooks.rs::door_command`'s claude wrapper must keep letting
  `SessionStart`/`UserPromptSubmit`'s stdout through** (`2>/dev/null` for
  exactly those two events; every other event, `Stop` included, keeps the
  original `>/dev/null 2>&1` swallow ON PURPOSE — don't widen either
  direction). Stdout is the only PIPE a `session hook` `Outcome` ever
  reaches the harness through (`aoide_protocol::door::run`'s `println!` on
  `Ok`) — reaching the harness is not reaching the model; which events
  Claude Code actually folds into context is
  `docs/Aoide-Wiki/protocol/dev/HARNESS-CLAUDE-CODE.md`'s call (its "Traps"
  section), not restated here, and it names exactly `SessionStart`/
  `UserPromptSubmit`. Restoring the old blanket swallow on those two
  silently kills every lane note without touching a single assertion in
  this crate's own test suite, since none of those tests observe the
  harness's actual stdout — only the returned `Outcome` in-process.
  Conversely, unmuffling `Stop` (or any other event) would only leak routine
  hook chatter into a channel nobody reads on that event — `on_stop`'s
  delivery is the pending-note relay through `on_prompt_submit`/
  `on_session_start`, never a wrapper change.
- **The hook door reads the hook's OWN claim exactly once, and never takes a
  pid from the wire.** `session_hook` is the only place that may read
  `AOIDE_SESSION_ID` for a parent (`own_parent_claim`), because it is the only
  place that knows whether this process IS the hook; the daemon arm takes the
  claim off `HOOK_PARENT_FLAG` and its pid off `DAEMON_PEER_PID_FLAG` — stamped
  by the door from `SO_PEERCRED`, never sent by a caller — and both arms resolve
  through ONE `resolve_parent_claim` (attested wrap first, then the claim, which
  is dropped when the claimed record carries a pid the hook process cannot be
  found under). Do NOT re-add a
  `std::env::var("AOIDE_SESSION_ID")` inside `hook_for_profile_gated`,
  `hook_ensure_session_with`, or either action arm: daemon-side that variable
  belongs to `aoided`, and a daemon launched from inside a conducted session
  would hand every hook-registered child that session as its parent. Do not
  stamp `hookAncestry` from `std::process::id()` either — use
  `window::hook_chain(door_pid)`, for the same reason — and do not give a
  `hook_pid: None` arm a fallback pid: no pid means `Unclaimed` and no stamp.
- **The undying transfer is one `save_undying` call, never two.**
  `resurrect.rs`'s `resurrect_one` adds the new id and drops the old one in
  the SAME in-memory `Vec<UndyingSession>` before writing — the new id goes
  in first, so a crash between the in-memory edit and the write leaves the
  OLD id undying (the next sweep retries it) rather than leaving neither
  undying (silent loss). The transfer only fires when the old id was
  actually undying (`is_undying` gates it) and only when the spawn reached
  `Status::Ok` — an ordinary `--all`/`--id` revive of a not-undying session
  must never start marking it undying, and a failed spawn must leave the old
  id exactly as it was. Don't split the add and the drop across two
  `save_undying` calls, and don't drop the check that the old id was
  undying. A `ByLog` candidate (`Candidate.by_log`) never transfers: its
  mark stays on the ledger id, and the `!c.by_log` guard is what keeps it
  there.
- **Bare `resurrect --project` selects the undying set, not a single
  "most recent" entry (P-C4, durable-sessions plan).** `resurrect.rs`'s
  `undying_selection` is the one place that reads `sessions.json` for
  liveness — the daemon's boot sweep (`aoide-server`'s `daemon.rs`) no
  longer runs its own liveness check before calling `session_resurrect`; it
  calls unconditionally for every `autoResume` project and relies on this
  function to drop already-alive ids per candidate. Don't reintroduce a
  project-wide liveness gate in the daemon loop — a multi-session undying
  set needs the exclusion done per id, not per project, or one live
  terminal suppresses reviving the rest of the set. `--all` and `--id` stay
  exactly as they were: neither consults the undying mark at all.
- **Bare `resurrect` (no `--project`/`--all`/`--id` at all) tries a project
  MANIFEST before anything else (U2, command-defrag lane U).**
  `resurrect.rs`'s `session_resurrect` walks up from cwd
  (`aoide_storage::manifest::walk_up`) and, on a hit, hands the whole
  outcome to `resurrect_from_manifest` — never falls through to
  `require_flag(inv, "project")` in that case. Any of the three flags
  present routes straight past the manifest check to the pre-existing
  flag-mode path unchanged; the two modes are mutually exclusive by
  construction, not by an explicit guard someone could accidentally widen.
  Don't make the manifest check consult `projects.json` — the whole point
  is that a manifest is self-sufficient on a host that has never registered
  the project at all.
- **The both-misses usage error is conditioned on having actually tried the
  walk.** `flag_mode` (`--project`/`--id`/`--all`,
  any one present) is computed ONCE at the top of `session_resurrect` and
  consulted a SECOND time at the `require_flag(inv, "project")` failure
  branch: `flag_mode == true` there returns `require_flag`'s own original
  `Err` untouched (a flag WAS given, `--project` just wasn't — the walk was
  never attempted, so the manifest-miss wording would lie); only
  `flag_mode == false` builds the "no .aoide/project.json above `<cwd>` and
  no --project/--all/--id given" message. A regression here (returning the
  both-misses message unconditionally on any `require_flag` failure) is
  exactly the bug this split guards against — `id_without_project_is_the_
  ordinary_missing_flag_error_not_the_manifest_message`/`all_without_
  project_is_the_ordinary_missing_flag_error_not_the_manifest_message`
  pin it down.
- **Every row of `resurrect_from_manifest`'s outcome carries a
  `disposition`.** `resurrect_one` is reused
  VERBATIM for the enrichment path and knows nothing about being called
  from manifest mode — none of its own pushes into `resurrected`/
  `skipped`/`failed` carry a `disposition` key. `resurrect_from_manifest`
  snapshots each bucket's length before calling it and stamps
  `"revived-from-ledger"`/`"skipped"`/`"failed"` onto whichever ONE grew
  afterward (`resurrect_one` always pushes into exactly one, never zero,
  never two, per call) — this loop's OWN pushes (`summon_remote`'s
  `summoned-remote`/`failed`, `clean_spawn_from_spec`'s
  `clean-spawned`/`failed`) already carry theirs inline. Don't add a new
  `resurrect_one`/`clean_spawn_from_spec`/`summon_remote` push path
  without also covering it here — an un-stamped row is exactly the defect
  a consumer filtering the outcome by `disposition` would silently drop.
- **A remote spec (`host` != this host) is `summon_remote`'s job, not a
  skip (U4, command-defrag lane U).** It resolves `spec.host` against
  `state/nodes.json` by node NICKNAME (the same nickname U3's picker writes
  a spec's `host` as), refuses LOCALLY into `failed[]` — never `skipped[]`
  — for an unregistered node, a registered-but-unverified one (mirrors
  `aoide-client::commands::handle_node_spawn`'s own local gate: an unsigned
  request can never satisfy the remote door's `Signature`-rung spawn gate),
  or nothing to summon with (`summon_text` returns `None`), THEN calls
  `aoide_client::commands::spawn_on_node` — never re-implement that wire
  call here, never shell out to the `aoide` CLI; the `conduct` → `client`
  edge is documented in `conduct`'s own `Cargo.toml`. `summon_text` never
  whitespace-splits a spec's `command` (there is no argv on this wire, only
  one prompt string) — don't reuse `clean_spawn_from_spec`'s split/rejoin
  logic here, it would collapse whitespace the operator wrote on purpose.
  The wire carries no cwd: don't add a `--cwd`-shaped field to the spawn
  request to compensate — that is a later phase's wire change, gated on the
  fleet's doors all running a binary new enough to read it.
- **Manifest-revived sessions are marked undying unconditionally, gated on
  `Status::Ok` alone — never on live registration (orchestrator design
  ruling, U2) — LOCAL paths only.** `mark_manifest_revival_undying`
  is called from `resurrect_from_manifest` itself, right after EITHER LOCAL
  path (enrichment via `resurrect_one`, clean-spawn via
  `clean_spawn_from_spec`) lands a row in `resurrected` — which by
  construction only happens past `Status::Ok`. `summon_remote`'s own rows
  never reach this call: the resurrected id lives on the node, and
  `state/undying.json` only ever names ids that live on THIS host — don't
  route a `summoned-remote` row through `mark_manifest_revival_undying`,
  it would mark an id this host has no authority over.
  This is its OWN `load_undying`/`set_undying`/`save_undying` call, NOT a
  `--undying` flag threaded into the shared `spawn` invocation: gating on
  `registered` (what `aoide spawn --undying` itself gates on) would make
  this unconditionally untestable in this crate — a live terminal
  registering is exactly the line `spawn.rs`'s own module doc draws as
  "never this crate's tests." Don't fold this into flag-mode's own undying
  TRANSFER block (`resurrect_one`'s pre-existing, untouched
  `is_undying(&c.entry.session_id)`-gated logic) — that block only ever
  moves a PRE-existing mark and must stay that way for an ordinary
  `--all`/`--id` revive; manifest-mode marks unconditionally because every
  manifest-mode spawn already came from an explicit, operator-authored
  declaration, not an ordinary revive.
- **The enrichment rule is a hard ordering: the manifest decides WHAT
  exists, the ledger decides HOW (the User's own design decision, U2).**
  `resurrect_from_manifest` never invents a candidate the manifest didn't
  name, and never lets the ledger override which specs get considered —
  it only ever enriches a spec that's already there, picking the NEWEST
  ledger entry whose `cwd`/`agent` match (host is never compared again at
  this point: a spec's `host` mismatch already skipped it earlier in the
  same loop, and the ledger itself is host-local state that is never
  synced, so every remaining entry IS this host's). A match reuses
  `resolve_candidate`/`resurrect_one` VERBATIM — don't fork a second
  harness/terminal-arm resolver for the manifest path. No match
  clean-spawns via `clean_spawn_from_spec`, which reuses `session_spawn`'s
  own windowed path — don't hand-roll a second spawn call here either.
- **A dry run decides with the launch's own helpers and launches nothing.**
  `--dry-run` has no rule of its own. Flag mode lists what
  `select_candidates` resolves, and `dry_run_listing` marks each row with the
  same `resume_argv` check `resurrect_one` makes. Bare mode decides each spec
  in `plan_spec`, with `resolve_spec_dir`, `enrichment`, `resolve_candidate`,
  `clean_argv` and `summon_check`, the functions `resurrect_from_manifest`
  calls too. Don't write a verdict into the dry path that the launch does not
  also make, and don't copy a helper. The parity test
  `dry_run_manifest_dispositions_match_the_real_launch_for_local_specs` pins
  the two together.
- **A dry run is never held or sent, on any door. From an unattested caller on
  a door that cannot approve, it gets what the call would give, minus the
  launch.** `session_resurrect_with` gives that: bare mode the refusal its
  launch gives; `--id` or `--all` with no `--project` the usage error
  `require_flag` gives, with no ledger, projects or manifest read before it (on
  MCP, the attestation check reads the sessions stage first); with `--project`
  the `dry-run-not-held` refusal, since the flag-mode launch would hold and show
  that caller nothing. `plan_spec` takes a non-local spec's verdict from
  `summon_check`, never from `spawn_on_node`. Don't route a dry run through
  `hold_whole`, `send_restore`, `launch_session` or the node client.
- **On an `--all` or `--id` call, only a dry run reads the undying set in
  `select_candidates`.** It computes `pickedByBare` (`undying_picks`) only under
  `dry-run`. A real call reads the set per revival, in `resurrect_one`, where a
  legacy `carry.json` migrates on the first read. Keep the picks read gated on
  the flag: a real call has no use for `pickedByBare`.
- **`pickedByBare` is per row, and `--id` takes the first ledger line for its
  id.** The bare form revives the newest ledger line of each undying id, so an
  `--all` row is picked only if it is that line. `--id` takes the first match
  in ledger order, which is the oldest exit, and it always has. On an undying
  id with two lines, `--id` lists the oldest and marks it not picked. The dry
  run shows the launch's own choice, so don't move `--id` to the newest here:
  that would change what a real `--id` revives.
- **A dry run's refusals audit under `dry-run`, at the points the real call
  audits.** `audit_resurrect` maps every status to `dry-run` while the flag is
  present. A usage error or a stage miss writes no line, on either path.
- **`resolve_spec_dir` (`aoide_storage::manifest`, U2) is the ONLY
  place a manifest spec's `dir` becomes a filesystem path.** It refuses,
  never clamps, a `..` that would resolve outside the project root after
  LEXICAL normalization — don't resolve a spec's `dir` by hand (a bare
  `project_root.join(dir)`) anywhere else; that would silently reopen the
  containment hole this function exists to close. It is a STRING check,
  not a filesystem one: a `dir` with no `..` at all can still pass through
  a symlink pointing outside the project root at USE time, undetected —
  accepted under the manifest's host-local, operator-authored trust model
  (the operator who writes a spec already controls their own disk), not a
  gap to close with a `canonicalize` call here.
- **The autogate reads a parent edge only through `send.rs`'s
  `attested_parent_of`.** A target's `parentSessionId` earns the parent-of-target
  and sibling rules only where its `attestedSpawner` names the same id. Only
  `session_conduct` writes that stamp, where the kernel backs the edge
  (`window::spawner_is_attested`). `session start --parent`, `graph link`, the hook
  door, and any moved edge leave no matching stamp, so that child is gated like
  any unrelated send. Every write of `parentSessionId` goes through
  `SessionRecord::set_parent_session`, which drops the stamp when the edge moves. A
  new producer of parent edges must do the same, and stamp the spawn-time field
  where the kernel backs the edge, or accept that its children are gated.
- **A parent automatically hears the children it spawned, and that line is
  the DAEMON's own, never a `send` (P-EIDOLON E5b, `docs/architecture/
  EIDOLON-TRACE.md`'s "Second slice"; the User's ruling, 2026-09-17).**
  `graph/pingback.rs::pingback` is the one implementation: called from `reap()`
  on the POST-LOCK side, AFTER `sync_eidolon_sessions()`, with its result
  deliberately NOT folded into `outcome.changed` (the `refresh_live_agents` rule
  — telling a parent something must not toast the desktop every twelve seconds;
  each delivery prints its own `[aoide/reap]` line instead), and only under
  `Door::Daemon` — every other door returns silently and lets the daemon's own
  tick do it. It is NOT routed through `session_send`: `send.rs` attests the
  sender from the running process's `/proc` ancestry, so inside the daemon the
  attested sender is the daemon and never the child, which is exactly why the
  reciprocal rule cannot be `sender_is_parent` made symmetric (do NOT thread a
  peercred pid into `send.rs` for it). The line is delivered the DOORBELL's way
  instead — `doorbell.rs`'s `connect_for_ring`/`write_channel` for a live
  channel socket (one write, close, no keystroke), else
  `send.rs::write_delivery` + the target's own profile submit key for a headless
  wrap, an interactive wrap with no channel skipped `interactive-composer`, and
  a wrap whose WRAPPED program is a shell skipped `shell-parent` (the channel
  arm is not a keystroke and is still taken for one) — with no gate, no
  `pending.json` entry, no provenance prefix, no `names_the_node` title rename
  (the line starts with `[`, and must never be routed through `session_send` for
  it), and no mailbase receipt. One audit line per delivery through
  `send.rs::audit_send` (widened `pub(in crate::graph)` for it, the
  `audit_pending`/`audit_resurrect` sibling precedent), gate label
  `autogate-child`, the delivered line as `untrusted_data`. **At-most-once,
  claimed before the delivery.** The per-child cursor is
  `state/stage/pingback.json` (`<child id> → {seen, silentAt?}` for a trace,
  `{hook: {phase, at, silent?, died?, route?}}` for a hook phase): a short
  `with_stage_lock` critical section reads it, decides, writes the advanced
  cursor (temp-then-rename), then clears from `hooks.json` the death facts it
  said; a failed cursor write or clear delivers nothing. The socket write
  happens AFTER, with no lock held, bounded by `connect_for_ring`'s own write
  timeout. A crash
  between the two loses a line, which is the safe direction (a duplicate is
  noise in a parent's composer); a child whose `agent:"eidolon"` record is
  gone drops out of the file on the same pass. Do NOT move the delivery
  inside that critical section, and do not add a second lock.
  **The hook source is the same lane, as a child module
  (`graph/pingback/hook.rs`).** A local-parented child of any registered
  harness that is not an `eidolon` (`hook::tracked`: no `remoteParent`, not a
  shell, sub-agent node, app thread or conducted wrap) is read off its
  `hooks.json` phase and stamp, never a trace, and its lines (`awaiting`,
  `silent 12 min · last: …`, `settled`, `died mid-turn · last: …`,
  `ended mid-turn · last: …`) join the SAME `claims.lines` the eidolon source
  fills, so `deliver` and its skips are one implementation. Its claim runs
  inside the
  SAME `with_stage_lock` section over the cursor `claim_locked` just built
  (`hook::claim`), reading the roster and `hooks.json` inside that lock and
  writing the entry's `hook` half — `{phase, at, silent?, died?, route?}` — so the two
  sources never read each other's half and one file write covers both; the
  hook children's ids join the tracked set, which is what keeps their entries
  from being pruned by the eidolon pass.
  **The recipient is the SPAWNER, never the child's `parentSessionId`.** A
  hook child's `parentSessionId` is its own HOST wrap (`HookAction::Start`
  resolves it from `attested_wrap(hook_pid)` or `AOIDE_SESSION_ID`), so
  delivering there rings the child's own PTY: a headless host takes the line
  as its next prompt (a self-prompt loop), an interactive one has it typed
  into the user's composer. `hook::recipient` climbs: (1) the host wrap must
  be a conducted wrap whose pid is in the child's `hookAncestry` (stamped by
  the hook door from the hook process's own peer credentials; `session start`
  cannot write it) AND whose daemon seal verifies — a seal re-derives the
  pid's start time, so a pid reused after the wrap died (its record lingers
  `done`, and `host_wrap` checks no state) or a pid hand-edited into
  `sessions.json` fails it; the key is fetched once per pass, before the lock, and no
  answering daemon means no recipient; (2) the
  wrap's `parentSessionId` is the spawner only while it equals the wrap's
  `attestedSpawner` (stamped by `session_conduct` from its own `/proc`
  ancestry, `window::spawner_is_attested`, set-or-cleared on every
  registration — a detached `aoide spawn` child is reparented to init, so the
  evidence cannot be re-derived later and must be stamped at birth, and a
  registration that happens only AFTER the spawner returned has none left to
  read and stays unstamped: fail-closed, never guessed; the stamp takes no
  seal, the hook-time check in (1) and (3) is the only one); (3) a conducted
  (sealed) spawner hears it, a hook-registered spawner is resolved to its own
  attested host wrap by (1).
  Never the child, never its host wrap, and a bare `session start --parent`
  edge, an explicit `--parent` the kernel did not back, and a wrap registered
  by `aoide wrap` have no recipient: do NOT accept a `parentSessionId` on its
  word, and do not widen this to a recipient the evidence does not name.
  Hold: a recorded death is said whatever phase the cursor last saw, and no
  latch gates it: `death` on a `done` record under 24 hours old is the unclaimed
  state, and `hook::claim` returns the child ids it said so the caller clears
  each fact (`hook::clear_said`) after the cursor is durable. The first
  examination of a child is a baseline and announces nothing, except a recorded
  death under a day old. `awaiting` is claimed on ENTERING the phase, not on
  every restamp, and carries no reason (`hooks.json` has none, and
  `set_session_state` clears `activity` on any non-`working` state, so a tool can
  never be named there); `settled` needs the cursor to have seen the child
  `working`/`awaiting` (the hour-later `stopped`→`idle` decay says nothing);
  `silent` is `>=` 12 minutes and under 24 hours of `updatedAt` age on a
  `working` phase, latched per stamp (a day-old `working` is a crashed record).
  There is NO inferred
  death on the gathered path: a dead harness under a sealed, live host is not a
  death, because the live host is the one that records it. The gone path
  (`hook::gone`) infers a death only once the host is gone (its record absent or
  its pid dead), whatever its seal says, with no status. Its rules, and the held
  rules that spare a recorded death from every prune (`held_deaths`,
  `unclaimed_deaths`), are stated once in `CONTRACTS.md` under
  `state/stage/pingback.json`: change that section and the code in `hook.rs` in
  one commit. The seal key is fetched whenever a hook
  candidate exists, a tracked record or a hook entry the cursor still holds, so a
  child pruned in the same pass as its wrap is still judged. Every value read
  back from the cursor is cleaned again where it is rendered, and the whole line
  passes `wire_line` at delivery, as every ping-back line does.
  **The wrap writes its own child's death.** `session_store::record_hook_deaths`
  runs from `session_conduct` (`graph/conduct.rs`) once the wrap's child has
  ended, immediately before the wrap ends its own record. Under the stage lock
  it writes the death of each hook child the wrap hosts: a child whose
  `parentSessionId` is the wrap, whose `hookAncestry` holds the wrap's pid,
  whose harness is dead, and whose hook phase still reads `working` or
  `awaiting`. Per harness, the newest hook id gets `phase: "done"` with the fact
  (`record_death`: the tool in flight as `lastTool`, the wrap's exit fact,
  `exit`, `signal`, or `timeout`, and the `spawner` the line goes to) in one
  upsert; the older ids of the same process are closed as `done` with no fact.
  The spawner is resolved here, while the wrap's own record still stands to
  climb through (`hook::recipient_of`: the route climb with no seal check, since
  the wrap holds no daemon key; the claim verifies it), so a child no tick saw
  before its wrap ended is still told. Each child's session is set to `done`
  (presence reads the process as over, and the held rules spare the record
  until the death is claimed). `aoide session phase` folds the phase it
  is given and never writes `death`. A closed phase is never written. The tick
  says the fact when the wrap wrote it, and infers a death only when none was
  written (SIGKILL), so each death is said once.
  **A session that ends itself mid-turn writes its own fact, and only itself.**
  `do_session_end_inner`, the one core of `aoide session end` and the hook
  door's `SessionEnd`, finds the child's hook phase still `working` or
  `awaiting` and `hook::recipient_of` naming a spawner, and writes the fact
  with `outcome: "end"` (no status), the tool in flight and that spawner in
  place of the plain `done`. The tick says it as `ended mid-turn · last: …`,
  latched and cleared like any recorded death. The wrap's later
  `record_hook_deaths` finds the phase closed and writes nothing, so one end
  is one line. `recipient_of` answers only for a child `hook::tracked` names,
  so an eidolon's death, which its trace says, is never told twice. A line to
  a spawner, an orchestrator agent, is an injection if a stranger can cause it,
  so the fact needs the caller's word, and the caller is the kernel's:
  `do_session_end` takes the attested pid (the hook door's `hook_pid`, the
  daemon's `__daemon-peer-pid` stamp read by `send::door_pid`, else this
  process; the wrap's own end passes its own pid, which its record names), and
  `runs_under_harness` writes the fact only when the child's harness pid is in
  that pid's `/proc` ancestry. A caller outside a live harness does not end its
  session at all (below). A `SessionEnd` whose `reason` the profile lists in
  `session_end_continues` (claude: `clear` and `resume`) ends the session with
  `attests_death` false, since the harness process lives on under another
  session id and nothing ended mid-turn: `map_hook` sets `continues` on
  `HookAction::End` and the door passes `!continues`. The reason is the raw
  payload field, matched exactly and never normalised, so `logout`,
  `prompt_input_exit`, `other`, a disguised `clear`, an unknown value and an
  absent field all write the fact; a harness whose profile lists none never
  skips on the field. A second `done` on a record that carries a fact is a no-op
  in `aoide_storage::session::upsert_hook`, so a repeated end or a duplicate
  `SessionEnd` cannot erase it; a live phase still supersedes the fact.

  **A live harness anchors its record.** The caller test above holds only if a
  stranger cannot first re-home the record onto its own chain, or end it so that
  `prune` drops it and a fresh registration takes its place.
  `session_store::live_harness_outside` (the record's session has not ended, and
  its `harness_pid` is alive and outside the caller's chain) is the one test
  behind both, and `session_store::live_harness_anchoring` is its read of the
  store for a caller's pid. `ancestry_may_move` refuses a chain move while it
  holds, whichever path asks; the hook door turns a `SessionStart` from outside
  the harness away whole, with `harness-anchored` as its reason and the session
  and harness pid in its message, and skips a per-turn hook's attested-parent
  re-stamp. A per-turn hook from outside is not refused: it still sets the
  phase. It fills or heals the record's chain only where no live harness outside
  its chain anchors the record (`ensure_session_chain`, through
  `ancestry_may_move`); the write moves no parent or fact.
  `session start` with a `--window` or `--parent` that would change the stored
  value, and `graph link` that would re-parent the record, are refused by
  `anchored_start_refusal` and `manage::link` with `harness-anchored`, unless
  the caller runs under the harness. A route needs the host wrap's pid in the
  child's own chain (`host_wrap`), so a re-parent alone draws no line.
  `do_session_end` asks the same question of the caller. When it holds, the end
  is refused by `anchored_refusal`: nothing is written (no `hooks.json` phase,
  no `sessions.json` state, no ledger exit, subagent cascade, tunnel close or
  check-lane baseline forget). The outcome begins `refused:` and carries
  `reason: harness-anchored`. `session_phase` sends a `done` phase to
  `do_session_end`, so the phase door is no way round the caller test. The reaper spares a record whose stored chain names a live harness (`reap::harness_alive`): the orphan arm and the staleness bands skip it, and only the pid signal of the record's own pid still judges it. When that pid is the terminal hosting the agent, a stale agent lives until the terminal closes: that is the accepted cost of pid-only liveness. A
  session that has ended (`state` `done`: its own `SessionEnd`, `/clear` and
  resume included), a dead harness, or a hook that runs under the harness
  releases the anchor, which is what lets `claude --resume` in another terminal
  adopt the id. That release is also the done-id squat, and the other same-uid
  residuals of this block (a descendant of the harness counting as it, the pid
  fill on a chainless record, the namespace-local probe, the wrap-end ordering)
  are listed under the trust model in `CONTRACTS.md`, not restated here. `hookAncestry` stores no start time, so a pid recycled by an
  unrelated live process holds the anchor until it exits; the refusal is the
  safe side. A new writer of `hookAncestry`, `parentSessionId` or a record's
  `state` on a hook-registered record goes through that test. The caller's
  ancestry is read when the end is served, so a harness that exited before its
  `SessionEnd` hook was served has left the hook re-parented: that end closes
  plain and its line is lost, never forged. Reading the ancestry at door entry
  would need `aoided` to stamp it at accept (a new wire stamp across the server
  crate) and would still only narrow the race, since the hook is already running
  when its harness dies, so the loss is accepted.

  **The wrap's own signals.** `WrapSignals::install` (`graph/pty.rs`) runs first
  in `session_conduct` and catches SIGTERM, SIGHUP and SIGINT with `SA_SIGINFO`
  and `SA_RESTART`. A signal that was SIG_IGN on entry is not caught (so an
  ignored HUP stays ignored, and the harness inherits it). The handlers are
  depth-counted: the last `WrapSignals` dropped restores every disposition it
  replaced. Only a signal whose `si_code` is `SI_USER` (sent by `kill(2)` to the
  wrap's own pid) is taken: `conduct_multiplex` forwards it to the harness once,
  SIGKILLs the harness after `WRAP_SIGNAL_GRACE` (5 s) if it has not exited,
  records the death with that signal as its outcome, and then ends with
  `die_by_signal` (the signal's default action, re-raised), so the parent sees
  the wrap die by that signal. Any other signal, such as one the kernel raises
  for the terminal, is re-raised with `SIG_DFL` and kills the wrap with no fact.
  Ctrl-C through the pty arrives as bytes on a raw console and never reaches this
  path (not tested directly). `poll(2)` returns `EINTR` whatever the flag says,
  and the stage lock's `flock(2)` retries on `EINTR`, so a signal landing mid-wait
  never leaves a stage write unlocked. SIGKILL is never caught: that death is
  inferred, and only the inference records it.

  **Trust model:** stated once, in `CONTRACTS.md` under
  `state/stage/pingback.json`. This cursor is unsealed data.
  Open gap, not checked here: a wrap that is SIGKILLed (no fact) whose child's
  record and hook entry are both pruned before any tick sees them is never said
  of. The inference reads that record, and an absent record drops the entry.
  **A live wrap owns its open child's end.** `reap_inner` takes out of its
  reaped set every hook child whose conducted host wrap still runs (and is not
  itself in the reaped set) with the child's turn open and a spawner to be told
  (`session_store::owned_by_live_wraps`, which shares `hosted_by` with
  `record_hook_deaths` and asks `hook::recipient_of` the same question), because
  a reap would close the phase before the wrap writes the death and the wrap
  would then find nothing to record. A child nobody would hear of is reaped as
  ever: the wait buys it nothing. Keep the hosting test one (`hosted_by`): a
  child the reaper spares must be one the wrap's exit can record. The cost is
  stated in `CONTRACTS.md` (`death`): a harness that dies under a wrap that
  outlives it reads `working` until the wrap ends. Shortening that wait belongs
  to the wrap (watching its own harness), never to the reaper. The hand-off is
  proved at two levels: `owned_by_live_wraps` as a table, and `reap` itself over
  a roster (`reap.rs`'s tests), since the `retain` that applies it is a separate
  line. The hook source emits no `PingEvent` and never spools: a child that
  belongs on a ring is `remoteParent`-stamped and so is not its child.
  **Two bounds on what the hook source says about deaths.** A stamp counts as
  recent only inside `[-SKEW_MS, STALE_MS)` of the clock (`hook::is_recent`, the
  one test `unclaimed_in`, `decide` and `gone` share), so a fact stamped in the
  future is neither said nor held, however far ahead. And `hook::claim` sends
  at most `DEATH_LINES_MAX` death lines per (host wrap, spawner) in a pass, then
  one `and N more ended or died mid-turn` line (`within_cap`), because the hook
  door lets an agent mint many ghost children in its own live wrap, and each
  would otherwise give its spawner a line at the wrap's end. Every fact is still
  named said, so it is cleared.
  **The screen source is the third, as a child module
  (`graph/pingback/prompt.rs`).** A local-parented conducted wrap whose
  harness has neither profile (`prompt::tracked`: `agent_profile` is none, and
  the hook and trace sources do not track it) is read off the `question` its
  own wrap published (below), and its `[<agent> <petname>] asking: "<line>"`
  joins the SAME `claims.lines`, claimed inside the SAME `with_stage_lock`
  section after `hook::claim`, into the entry's `screen` half — the last
  question announced. There is no baseline (the wrap publishes once its line
  has stood for 3 s), a different question is a new episode, the same one is not
  heard twice, and a record that no longer holds a question while `awaiting`
  clears the key. A question that carries `needsSudo` is never claimed
  (`prompt::asking`): the password is the human's. The child IS the wrap, so
  there is no host to climb from:
  `prompt::recipient` requires a daemon seal over the wrap's own pid, then the
  spawner climb the hook source uses (`hook::spawner_of`, steps (2) and (3)
  above) — one climb, two callers; do not fork a second. The seal key is
  fetched before the lock for a pass that has a `prompt::pending` record, as
  for a hook one. The question is cleaned and quoted again at render: it was
  capped where it was written, but this is the line a parent types.
  **The event and the line are two things, and a remote parent gets the
  event (P-RSA S8, `CONTRACTS.md` §4/§6).** `choose_event` decides WHICH
  `PingEvent` a child's new records amount to — a closed enum whose every
  string is already `clean`ed at the sender — and `render_line(tag, &event)`
  renders the ONE line from it; the tag belongs to whoever renders, so an
  event names no node. Local lines are unchanged to the byte, which the
  module's own fixtures pin. A child whose record carries `remoteParent` (the
  A2A door's stamp, whatever its harness) SPOOLS its events to
  `state/stage/pingback-remote.json` through `aoide_storage::pingback_remote`
  — cap 16 per child, monotonic `seq` per child, a read that missed any says
  `gap` — stamping the ring entry with that stamp's KEY (the far door's only
  gate input once the record is gone) and with the tick's own `at`. `deliver`
  is NEVER called for it: the far parent pulls them off this node's own door.
  The same pass drops the rings no parent can be owed any more
  (`retain_rings`: the child's record is gone AND the ring's `at` is past
  `RING_GRACE_SECS`), because a ring is rewritten whole on every spool and an
  unbounded set of dead children would be paid for by every live event.
  `PingEvent::Exited` is the one row that comes from the
  RECORD (`state` folds to `done`, plus its `outcome`/`exitCode`) rather than
  from the trace, it is claimed ONCE per child (the cursor's `exited` latch —
  a child that keeps no trace has no `seen` to advance past its own end), and
  it fires only for a remote child (Q5's ruled default: a local parent hears
  the run's own report). It sits AFTER every trace row and takes the place of
  the silence row, so the last thing before an exit is the child's own last
  word. A child that leaves the roster is never decided again — its record is
  gone, so its cursor entry leaves on that pass — so a dropped REMOTE child
  claims its trace row AND its exit on that one pass, and the ring always ends
  with exactly one `Exited`.
  **Typed only by a positive proof** (house rule 4). A line submitted into a pty
  RUNS if that pty is a shell, and `agent` is only a label a caller picks:
  `--agent pi -- bash` names a registered profile over a pty running a shell,
  which is how a ping-back line came to be executed (N1). The proof is
  `conduct.rs::is_its_harness`: the record's `wrapped_program` (stamped at
  registration from `conduct.rs::wrapped_program_name`, the launcher walk's
  resolved program) must equal the first launch token of the profile its `agent`
  names. Two automatic readers ask one refusal, `conduct.rs::auto_type_refusal`
  (a shell first, then the `is_its_harness` proof), and none restates it: the
  ping-back (`pingback.rs::unreceptive`) and the doorbell's PTY arm
  (`doorbell.rs`). The A2A spawn's prologue asks its argv twin,
  `conduct.rs::wrapped_harness`, the same walk read before a record exists. The
  refusal names and their order, the proof's limits and the one-time cost live
  once, in `CONTRACTS.md`'s `wrappedProgram` paragraph. Locally, each launcher
  resolves through its own option table (`conduct.rs::launcher`), and an option
  the table does not list refuses the whole launcher: the walk never guesses how
  many tokens an option takes (default-deny). The tables are coreutils 9.11,
  util-linux 2.42, sudo 1.9.17 and bash's `exec`; `doas` is from its man page.
  `script` and `su` answer by a scan for `-c`/`--command`, which only
  over-matches. `nix` resolves `develop|shell -c`, and `env -S` refuses (its
  split string is a command line). Every other launcher names itself. `shell`
  and `wrapped_program` are written together at registration on both
  `upsert_session` arms, so re-registering an id flips both. Shell syntax in a
  child's text (the question, the tag, the say, the prompt, the stop reason, the
  tool label) is made inert on the way into a line (`pingback.rs::inert`: `$(…)`
  becomes its fullwidth twin), and every line takes one more pass as it is
  written (`pingback.rs::wire_line`, the only place `deliver` shapes the bytes).
  That pass is the backstop for a field that skipped its own cleaning. It takes
  `!` and `"` with no carve-out, so the renderer's own quote and `! ` marker
  reach the wire as their fullwidth twins. What the basename proves, and what it
  does not, is stated once, in `CONTRACTS.md`'s `wrappedProgram` paragraph. The
  walk reads argv only. Those four predicates (`shell-parent`,
  `unverified-wrap`, `not-conductable`, `parent-done`) live in ONE function
  (`unreceptive`), which `deliver` itself calls, so nothing may restate them.
  The LOCAL parent/sibling autogate in `send.rs` is deliberately not this rule:
  that caller is a resident session steering a shell it can see in its own
  roster, no remote text rides its line, and an agent that hand-relays a peer's
  words into it is steering with its OWN authority — by design, and not
  something a second guard here could fix. Conducting shells is the product's
  headline. **The parent pulls its own remote children (P-RSA S9, `CONTRACTS.md`
  §4/§6).** `pingback_pull` runs in the SAME post-lock block, right after
  `pingback`, and only under `Door::Daemon`: for every row in
  `state/stage/remote-children.json` that is not `drained` and whose
  `parentSessionId` is a record `unreceptive` admits, it asks that child's node
  `tasks/get` + `aoide/linesAfter` = the row's cursor
  (`aoide_client::commands::task_history_on_node`, tunnel key = the PARENT's
  session id, so the ssh forward is the parent's own and the daemon holds none)
  and renders the answer HERE. The far node is resolved by the row's `key`
  alone, never by its display `node`. The events are a peer's bytes: each is
  re-validated against the closed set (`ping_event_of` — an unknown kind is
  dropped), every string re-cleaned (`reclean` = the shared
  `common::strip_unsafe` + `SAY_MAX` + the `/`/`!` guard, because the BARE
  segments `render_line` emits are never quoted) and every number re-clamped
  (`COUNT_MAX`/`MINS_MAX`, and an exit code outside `0..=EXIT_CODE_MAX` is
  DROPPED rather than clamped into a fabricated `exit 0`), then rendered by the
  LOCAL `render_line` under `[<node>/<child id>]`. **The claim is atomic, not a
  read-then-write.** The cursor is advanced through
  `remote_children::claim_lines_after` — one `with_stage_lock` section that
  reads the stored value and advances it, returning the cursor the caller may
  deliver from — and only events with `seq` past that are delivered; two
  overlapping passes therefore cannot duplicate a line, and a claim that cannot
  be written delivers NOTHING (losing a line is this lane's safe direction,
  duplicating one is not). `last` is trusted ONLY under `gap`, where the ring's
  own doc says it is the resync; without a gap an honest ring cannot report a
  `last` beyond the events it sent, and believing a peer's number there would
  silence the child forever. A `gap` buys exactly one arithmetic marker line,
  and only when the claim left a hole to name. **The pass has a budget and a
  rotation.** `PULL_BUDGET` (6 s) bounds the whole loop, because the rows are
  pulled synchronously inside the daemon's tick and a node that is merely off
  burns its request's full timeout; rows past the deadline are named
  (`budget-spent`) and audited once, and `PULL_ROTATION` moves the pass's
  starting row one along every time, so no row starves. A drained `Exited`
  latches the row with `remote_children::mark_drained` — and so does a PERMANENT
  answer (`TASK_NOT_FOUND_CODE`: the far node holds neither a record nor a ring
  for that child, so it is gone for good and retrying it forever would only grow
  the audit log). A refusal or a transport failure is NOT permanent: both stay
  retryable, from the same cursor, with no backoff — one pull per child per
  tick. `deliver`'s gate label is a parameter: `autogate-child` for a local
  line, `autogate-child remote` for a pulled one. `clean`/`quote`/`reclean` all
  judge through `common::strip_unsafe` — control, Unicode `Cf`, AND the
  invisible non-`Cf` fillers (`is_invisible`: variation selectors, Hangul,
  braille, Mongolian and Khmer fillers, U+034F), and the line and paragraph
  separators — and a new sanitizer anywhere in this crate must too, never
  a copied table (S9's own review found the local lane filtering `is_control`
  only, and the fillers missing). The table and its filters are
  `aoide_protocol::untrusted`'s, re-exported through `common` so these paths
  stand, and `graph::clean_line` re-exports `clean_line` for the SAME reason
  (P-RSA S10 review, L6): the A2A door echoes a caller's illegal slug in its
  refusal and must clean it with this one, not with a second rule of its own.
  **Both ends of a child's exit are one input.** `pingback`'s `dropped` slice
  carries the eidolon sync's own drop set AND (H1 of the S8/S9 review) the
  end facts of every remote child `reap_inner` took off the roster this same
  pass — the kill path, the prune, the superseded tombstone — because the
  roster is re-read after the sweep and a record that left before that read is
  otherwise never decided over. One pass yields at most ONE event per child
  (the dropped loop dedupes by id, and the exit `decide` already answered is
  never pushed a second time): a ring closes with exactly one `Exited`.
  **`died mid-turn` is the sync's own additive return.**
  `sync_eidolon_sessions` returns `(bool, Vec<DroppedEidolon>)` — the bool
  means exactly what it did before, so every existing caller is unchanged in
  behaviour; the second half names the records that pass removed (id,
  petname, parent edge, agent, and the trace `TranscriptSpec::locate`
  resolves for it). Do NOT add a second liveness probe for the dead-pid row:
  `reap` is the only sweep (`AGENTS.md`'s standing rule), so the ping-back
  consumes that sweep's own drop list and probes nothing itself.
- **The ping-back reuses `graph/trace.rs`'s renderers and the roster's own
  extractors, never a second formatter.** `one_line_clip`/
  `tool_result_summary` (widened `pub(in crate::graph)`) render the clipped
  phrase and the tool label `session trace` already shows; `say` comes from
  the harness CAPABILITY `TranscriptSpec::say` — the very function that fills
  the roster's `say` field — and the trace path from `TranscriptSpec::
  locate` + `TranscriptSpec::trace`, never `if agent == "eidolon"`. The
  silence row reuses `graph/eidolon.rs::eidolon_state_from_trace` for "the
  turn is still open": one state fold in this crate, not two. Every
  child-authored fragment goes through ONE `clean` (house rule 4): one line,
  control characters stripped (a `\r` is an Enter at a headless PTY),
  clipped to 80 chars with `…` — the tool label on the failing/silence lines
  included, not only what the grammar quotes; quoted fragments additionally
  get a prefixed space before a leading `/`/`!` so they can never read as a
  command or an escape at a parent's prompt. A new fragment that carries the
  child's words joins `clean`, never a second stripper.
- **`reap` (toast-free) and `reap_and_announce` (the registered CLI/daemon
  handler) are deliberately two functions, not one.** `reap_and_announce`
  spawns a REAL `notify-send` on the live desktop whenever the sweep
  changed anything (or `--announce` is passed) — every in-crate caller and
  unit test calls the bare `reap` instead, and MUST keep doing so; a test
  that dispatches the real `session reap` command path (proving daemon/socket
  routing, not sweep logic) reaches `reap_and_announce` for real and has to
  neutralize `notify-send` itself (e.g. blanking `$PATH` for that one call)
  rather than letting a test fire a real toast on the machine running it.
- **A protected live wrap is ended only by its own exit, and nothing reads the
  daemon key on that path.** The predicate is
  `identity::is_protected_live_wrap`, stated once in `CONTRACTS.md`'s identity
  section. Every refusal asks it: `session end`, a terminal `session phase`,
  every hook arm (below), `session prune`, the reaper's spare, and `graph link`
  (as the child). A single id is read through `session_store::protected_wrap`,
  never a second copy of the predicate. The start door (`session_store`'s
  `session_start`, after the daemon hop) and the hook door's SessionStart
  (`do_hook_session_start`) both ask it before any write, through one
  verdict (`live_wrap_verdict`), and a refused hook start is audited on the
  door the hook arrived on. A start judged on a protected wrap writes
  nothing, refused or not, and the hook door returns that judgement beside
  its outcome so `send.rs` lands no stamp after it. `drop_sessions` keeps a
  protected survivor's `parentSessionId` and drops its `attestedSpawner` when
  the record its edge names leaves the roster, and `do_subagent_rekey` keeps
  the edge too. `graph link` and `session prune` load and write under the
  stage lock. `identity::live_sealed_wrap_ids` is the set the doors and the
  reaper test. The wrap's own exit path (`do_session_end` from
  `session_conduct`) is the one legitimate end and stays unrefused. A new door
  that can finish or re-register a record asks the same predicate first, and
  the wrap's own exit path never does. The grant side (`send.rs`'s
  `real_attested_*`, `actions.rs`'s `terminate_verified`, `pingback.rs`) still
  verifies the seal, and must.
- **No hook arm acts on a protected wrap's own id, and the guard is one
  place.** A harness reports under its hook child's id, never its wrap's, so
  `hook_for_profile_gated` asks `protected_wrap` for the payload's `session_id`
  right after `map_hook` (before the unmapped-event stamp as well) and returns
  `noop("live-sealed-wrap")` for every event but SessionStart, after one audit
  line on the door it was given (`audit_ignored_wrap_hook`; the line names the
  wrap and never the payload). SessionStart stays distinct on purpose: its arm
  judges the four identity fields against the same record, answers `ok` for a
  start that changes none and refuses one that does, and a same-fields start is
  not audited. Never add a per-arm protected check (the guard covers the
  `SessionEnd` arm too), and never move a stamp, a window or chain write, a
  phase write or a `spawn_summons` call ahead of the guard. A new arm is
  covered by being below it. Tests that forge a hook at a protected wrap set
  `AOIDE_AUDIT_LOG`, since the guard appends to it.
- **A payload field that names a `sub:` node is guarded where it is written.**
  The guard above reads `session_id` and nothing else, while `tool_use_id`,
  `parent_tool_use_id` and `agent_id` build the `sub:` ids that
  `do_subagent_spawn`, `do_subagent_end`, `do_subagent_rekey` (its source id)
  and `set_owner_activity` write under. Each of the four asks `protected_wrap`
  for that id inside the stage lock and returns when it names a protected live
  wrap, so a wrap registered under the prefix before it was reserved is not
  re-kinded, renamed, dropped or given a phase or an activity by a forged
  payload. A new writer keyed by an id a payload names asks the same reader.
- **The `sub:` id prefix belongs to the sub-agent hooks.** `do_subagent_spawn`
  creates a `sub:<id>` node, `do_subagent_rekey` renames it and
  `do_subagent_end` removes it, each on a harness's word alone, so a wrap or
  session registered under that prefix would answer to a forged hook.
  `session_store::reserved_id_refusal` (a taught `Kind::Usage` refusal, reason
  `reserved-id`) is asked by the four doors that take an id from their caller:
  `session_conduct`, before the child is spawned; `session_spawn`, before any
  mailbox enrolment, sidecar or launch (its child would refuse the id only
  after all three); `session_start`; and a hook's SessionStart, as the Start
  arm's first act, ahead of `clear_stale_parent` and every other write, and
  again in `do_hook_session_start` for the first hook of an unknown id. The
  A2A door mints its own id (`spawn_session_id`) and takes none from its
  caller. A new door that registers a caller's id asks it too. The eidolon and
  Codex-app reconcilers read the id off disk instead, so they skip an id for
  which `is_reserved_id` holds: they enrol none and retire one enrolled under
  it. Together these make the sub-agent hooks the only makers of a `sub:` node.
  It is why a `sub:` record is never a spawner (`window::spawner_is_attested`
  needs the parent's pid in the ancestry, or its hook chain), so no stamp is
  ever left naming a sub-agent node that ends, is re-keyed or is cascaded away.
- **A session's exit — a clean `session end` OR a `reap` sweep — MUST
  append exactly one line to the durable session ledger, through the SAME
  shared call (P-D8, `docs/architecture/AOIDED.md`'s "L5").**
  `graph/doc.rs::ledger_session_exit` is that one call;
  `session_store.rs::do_session_end_inner` gates it on the record's own
  `state != "done"` BEFORE mutating (a re-run `session end` on an
  already-done id must never double-append — `do_session_end` never prunes
  its primary record the way `reap` does, so a second call on the same id
  is a real, reachable case), while `reap_inner` needs no such gate since
  every id in its `reaped` set is already filtered to `state != "done"` by
  construction and gets physically pruned within the same call. Don't add
  a second ledger-append call site for a new session-write handler — route
  it through `ledger_session_exit` (re-exported `pub(crate)` at `graph.rs`
  specifically so `reap.rs`, a sibling module, can reach it) the same way.
- **This crate's own `env_lock()` (`lib.rs`)'s first call in a test binary
  also floors
  `$AOIDE_DAEMON_SOCKET` at a path nothing could ever listen on, unless a
  test already set one (P-D6 safety net — an incident, this phase: a real
  resident `aoided` on this exact dev box shares the default socket path
  every routed test handler resolves to when unset, and a test that forgot
  its own override silently mutated PRODUCTION `~/Aoide/state/stage/
  sessions.json` through it before this floor existed).** Don't remove or
  weaken this floor to "simplify" `env_lock()` — a test that WANTS to prove
  real daemon routing still installs its own `$AOIDE_DAEMON_SOCKET`
  override afterward, same as any other env var here. **A second floor
  (P-D8, same reasoning, one env var over) does the identical thing for
  `$AOIDE_STATE_DIR`:** `session_store.rs`/`reap.rs` had zero prior
  references to it before the ledger write landed, so without this floor
  every `session_end`/`reap` test in either file would silently append
  into the REAL `~/Aoide/state/session-ledger.jsonl` on this box the
  moment its own test forgot (or never needed) an override. Same rule:
  don't remove it, and a test proving something about the ledger installs
  its own `$AOIDE_STATE_DIR` override on top, same as any other env var.
- **A nested headless session is windowless BY CONSTRUCTION — never
  pid-ancestry-walk it to a window (task #89).**
  It is NOT enough to gate the backfill call sites alone — a
  headless wrap's OWN record needs the same protection its descendants get,
  or the whole mechanism re-poisons on a live compositor. The parts, each
  required:
  - **The registration gate.** Registration (`graph/conduct.rs::session_conduct`)
    never reads a window, so a `--headless` wrap has nothing to discover. The
    permanent `headless: bool` marker is still written there, through
    `session_store::stamp_headless` (change-only, written once, never cleared),
    right after registration.
  - **The chain and sweep gates.** `window::windowless_by_lineage` checks a
    session's OWN record FIRST (via `is_windowless_wrap`: `conductable` and
    (`headless` OR its own `windowAddress` empty)) before walking its parent
    chain, so the wrap's own record is caught by the same function that catches
    its descendants. Two callers route through it: `ensure_session_chain`, which
    returns early, and the desktop's sweep (strillix `hypr/sweep.rs`), which skips
    the record. Miss either, and a nested `conduct --headless`/`spawn` picks up a
    chain the sweep can bind to the ENCLOSING terminal's window. That is what made
    the same-window eviction (below) treat an agent and its own headless
    grandchild as stale twins.
  - **Open: `session window attach` does not re-check lineage.** The verb trusts
    its caller, and its one automated caller, the desktop's sweep, applies the gate
    above first. A core-side check cannot reuse `is_windowless_wrap` as it stands,
    because that predicate also marks an unwindowed conducted wrap, which the
    desktop must still be able to bind after registration. The rule for that case
    is undecided.
  - `headless` deliberately overrides a stray non-empty `windowAddress`:
    it is a PERMANENT self-reported registration fact, not a live re-check,
    so a bug elsewhere that stamps a window onto a headless wrap's record
    still can't make `is_windowless_wrap` say otherwise.
- **A window hosts any number of agents; a same-window record is evicted
  only as a superseded twin (`session_store::superseded_by`).** One desktop
  app window runs six headless claudes, and one kitty window runs several
  tabs. The shared window address never decides eviction by itself. Two
  carve-outs come first. The new record's WHOLE lineage is never evicted
  (`session_store::lineage_of`, ancestors + descendants, walking
  `parentSessionId`: a nested `conduct`/`spawn` chain can land a same-window
  grandparent/grandchild pair).
  A conducted PTY host (`conductable`) is never a candidate. Past those,
  `superseded_by(old, chain, pid, alive)` retires `old` only when (a) no process of
  its own (its chain or its pid) is alive outside the new record's chain, and
  (b) it is a legacy record with no chain that holds a pid (a chainless record with no pid is never a twin), or its first shared chain pid sits
  at the same depth in both chains (the same harness position under a new id:
  `/clear`, a compact, a same-tab successor). A nested run beneath a live
  harness is never a twin. `do_session_start_inner` (registration) and
  `reap`'s dedup pass (`superseded_agent_duplicates`) call that ONE rule, so
  they cannot disagree. Do not reintroduce a window-only test in either
  pass: a live sibling sharing the window would be retired.
- **A window is a fact the desktop reports, never one core discovers.** Core
  reads no compositor. The desktop's sweep (strillix `hypr/sweep.rs`) places a
  session's window and writes it through `aoide session window attach`, the one
  core verb that records a window. It fills an empty window, moves the workspace
  when the same window reports again, and refuses a second window while the first
  is held (`Attach::Held`). Core never stamps an owner's pid on a session: a
  window's owner is the app, not the session's process, and a stamped owner pid
  made every subprocess of the app attest as that record. A self-reported `pid` is
  vouched only when it is one of the hook's own ancestors
  (`send.rs::vouched_payload_pid`), so a payload may name its harness and never the
  app that owns the window.
- **A pid-less hosted session binds only through a live chain, and the desktop
  makes that walk.** The desktop's sweep (strillix `hypr/sweep.rs`,
  `window_for_hook_chain`) walks a record's stored `hookAncestry`: the nearest
  chain pid that owns a client is the owner, and the record binds while any
  process of its own chain, a tab shell included, stands beneath that owner. A
  dead chain binds nothing (a dead pid's ancestry is itself), and a reused pid
  outside the owner's tree binds nothing. Core never performs this walk and never
  writes a pid for such a record. A chain with no live process beneath the owner is
  replaced by the next per-turn hook from its live harness (`ensure_session_chain`),
  and then binds at the desktop's next sweep.
- **`RestoreSnapshot.idle` is captured as its OWN field, never inferred
  from `state` (P-C5).** `reap.rs`'s sweep assigns `s.state = "done"`
  BEFORE calling `ledger_session_exit` — by the time the ledger line is
  written, the pre-exit idle/working state is already gone from `state`.
  `idle` is `shell_at_prompt`'s verdict, computed ONCE per tick in
  `conduct_refresh_shell` and handed to both `shell_snapshot` (for `state`)
  and `restore_snapshot` (for the field), so the two cannot disagree. It is
  stored independently, so a later reader (the ledger, and eventually a
  resurrect consumer) never has to reconstruct it from a field the reap path
  has already overwritten. Don't recompute it inside either snapshot.
- **A command shell is decided by `shell_at_prompt`, never the pgid alone.**
  `bash -c "sleep 30"` exec-optimizes, so the conducted pid becomes `sleep`
  and the pgid check reads it at the prompt. An interactive shell is the pgid
  check alone; keep it that way. The reads and their order are CONTRACTS.md's
  shell-state paragraph.
- **A privileged session is never idle, and nothing is read for it.**
  `ShellWatch::privileged` holds the conducted argv when
  `runs_a_privilege_program(&inv.args)`, and `shell_at_prompt` answers working
  on it before its first read. Keep that check first: a stand-in that is not
  setuid passes every later read as idle at its password prompt, and an idle
  verdict would read the root prompt as a shell at rest. Its `activity`
  (`command_label`) and `restore.argv` come from that line, never from
  `running_pid`: `doas` execs into the root shell and `su` hands the
  foreground to its child, so the pid would name a root command, which
  resurrect would then type. The rule reaches exactly `sudo`, `doas` and `su`,
  and what they wrap: only these read as a shell, and only a shell has an idle
  verdict or a restore. `run0`, `runuser`, `pkexec`, `sudoedit`, `setpriv`,
  `chroot`, `sg`, `newgrp`, `dzdo`, `ksu` and `busybox su` are non-shell wraps
  with no restore, so they are not detected here. `PRIVILEGE_PROGRAMS` is the
  restore refusal's list, wider than this rule's scope. The accepted losses,
  the no-pty one among them, are in CONTRACTS.md's shell-state paragraph.
- **The executable baseline is the file the spawn started, read from its path
  (`resolve_program`, once; `PtyChild::image`; `shell_baseline`), never from
  `/proc`.** A `/proc/<pid>/exe` read can land after the shell has exec'd its
  command, and then the command is the baseline. The spawn starts that same
  path with the name as argv[0], so the baseline and the exec are one file. The
  CPU sample is taken on every tick. Don't move either read into
  `shell_at_prompt`. `busybox` is known by the canonical name of that file,
  never by argv[0], so an `sh` that is a symlink to it has no baseline. A hard
  link named `sh` is not caught.
- **`proc_argv` and `proc_command` are deliberately two functions, never
  merged (P-C5).** `proc_command` is a DISPLAY label — it basename-
  collapses `argv[0]` and truncates at 48 chars — built for the roster's
  `activity` column. `proc_argv` is a re-exec CANDIDATE — raw, uncollapsed,
  unclipped `/proc/<pid>/cmdline` — built for `RestoreSnapshot.argv`, which
  a later phase re-execs. Reusing `proc_command` for `argv` would re-exec
  the wrong binary (a collapsed `argv[0]`) or a truncated one; don't share
  the two even though they both start from the same `/proc` read
  (`parse_cmdline` is the pure split they DO share).
- **Shell-likeness is derived from the WRAPPED ARGV, never the display
  name (task #100, the P-C7 soak's live finding).** `session_conduct`'s
  `is_shell` — which gates the ENTIRE P-C5 refresh/capture path (cwd
  tracking, working/idle state, foreground argv, the restore snapshot) —
  comes from the SAME `conduct.rs::program_is_a_shell(&inv.args)` the refusal
  lanes read
  (`program`'s own basename against the shell list, with the launchers a
  shell arrives through resolved first). It is NOT `agent ==
  "shell"` — `agent` is a caller-chosen label (`--agent <name>`, or the
  command's own basename by default) that can disagree with what is
  actually conducted on purpose (a soak harness, an experiment). `spawn
  --agent soak-a -- bash` is the exact live shape that broke under the old
  string compare: a real interactive shell whose roster record never
  ticked, because its label wasn't the literal string `"shell"` — marking
  it undying yielded a ledger entry with `restore: null`, so resurrect
  reopened nothing but a default cwd. Don't widen this back to a name
  compare "for simplicity" — a caller is always free to label a shell
  anything it likes, and the capture path must not depend on that choice.
  kitty.nix's own terminal wrapper needs no separate case: it always execs
  the resolved login shell explicitly as the conducted command, so its
  basename satisfies the predicate the same way any other shell invocation
  does. `undying.rs::nothing_to_restore_warning` is the mark-time corollary
  — see its own bullet below.
- **A restored line is NEVER submitted, and only a busy session gets one
  (P-C6, durable-sessions plan) — this crate's single most important
  resurrect invariant.** `resurrect.rs::restore_delivery` returns a
  `session_send` flag map only for a session that was demonstrably RUNNING a
  foreground command (`idle: false`) whose argv `restore_argv_refusal`
  passes, with `--yes` and no `submit`. Don't add `submit` to it. An argv is
  attacker-choosable, and reading one cannot prove what it will do (a
  password prompt, a destructive command); the human who reads the typed line
  presses Enter. An idle snapshot returns `None` and only its cwd restores.
  Don't add a branch that delivers idle text: nothing typed in an old session
  is carried forward, so there is no stale line that could fire at boot.
- **Restore delivery is SELF-ATTRIBUTED (`--from <new-id>`), and
  `send.rs`'s self-attribution rule delivers such bytes verbatim — both
  halves stay, together (P-C7 live finding #2).** `send` prefixes a
  delivered payload with `from <sender>: ` for attribution; restore rides
  `send`, so the live soak's resurrected re-exec arrived as `from
  quiet-birch (…1892): /run/…/sleep 900` — a bash syntax error. The fix keys
  off the ATTRIBUTED sender (`resolve_sender`, i.e. `--from`), never the
  gate's own sender identity
  (LANE IDENTITY P-ID2: a kernel-attested session, `graph/identity.rs::
  attested_sender` — the two are separate axes on purpose, module doc):
  restore is delivered from ANOTHER session's env, which is exactly how it
  got mis-prefixed. Removing the `from` flag from
  `restore_delivery`, or the `attributed_to_target` suppression in
  `deliver_local`, silently reintroduces the corruption. Not a gate
  widening: `--from ""` (explicit anonymous) already skipped the prefix,
  and the audit line records the attributed sender either way.
- **A SHELL target's delivered bytes are ALSO always verbatim, for every
  sender, not only a self-attributed one (#116, the same "delivered bytes
  arrive verbatim" discipline the bullet above holds).** `deliver_local_with`
  gates the `provenance_prefix` call on `rec.agent` being neither `"shell"`
  nor empty (the same shell-shaped check `profile_for_agent`'s own fallback
  already treats as equivalent) — a shell has no concept of an attribution
  comment on its input; whatever reaches the socket is read as a COMMAND
  LINE, so `from <sender>: rm -rf /tmp/x` corrupts the command exactly like
  an unprefixed restore delivery would have. An AGENT target (any other
  `rec.agent`) keeps the prefix — a prompt is not a command line, and the
  agent benefits from seeing who sent it; don't widen this check to agents
  "for consistency." The audit line records the attributed sender
  regardless of target kind — this invariant only ever governs the bytes
  written to the socket.
- **The submit keystroke is a SEPARATE, LATER socket write, never
  concatenated onto the text payload (task #124, live-diagnosed on kimi
  0.31.1).** `send.rs::write_delivery` writes `payload` first, sleeps
  `SUBMIT_KEYSTROKE_DELAY`, then writes `submit_key` alone — a `\r`/`\n`
  arriving in the SAME pty write as preceding text is what kimi's TUI input
  parser paste-coalesces into a composer newline instead of Enter, leaving
  the prompt unsubmitted; a keystroke arriving as its own later write
  submits correctly. `conduct_multiplex`'s injection relay (`graph/
  conduct.rs`) already does one `read()`-then-pty-`write()` per `poll()`
  wakeup, so this needed no relay-side change — only the WRITER side had to
  stop concatenating. That relay shape is a timing argument, not a message
  boundary: SOCK_STREAM carries none, so a relay thread starved past the
  delay would read both writes back as one concatenated chunk — the delay
  IS what keeps the two writes distinct at the pty. Boot auto-resume's restore
  resubmits pay it beyond the CLI, and they run serially before the
  daemon's tick loop starts — N undying resubmits delay reaper start by
  N×300ms, bounded and boot-once. `SUBMIT_KEYSTROKE_DELAY` (300ms) is empirically
  pinned, not guessed: a live windowed kimi session on this box reproduced
  the exact newline-not-submit failure at 120ms and submitted cleanly,
  twice, at 300ms — don't shrink it back down without repeating that live
  proof. Applied to EVERY profile, one code path, no per-agent branch: a
  separately-written `\n` is semantically identical to the old concatenated
  one for claude/pi, so this is a delay, not a behavior change, for either.
  Don't reintroduce `payload.push_str(submit_key)` before the write — that
  is the exact regression this invariant guards against.
- **A recorded foreground argv is re-typed only when `restore_argv_refusal`
  clears it (P-C6).** The refusals are listed once, in CONTRACTS.md's restore
  paragraph. A new refusal goes in that function with its test, never into a
  second list beside it. `PRIVILEGE_PROGRAMS` is the one list of privilege
  programs. `shell_join`'s bare-safe set keeps `=` (`spawn`'s `{cmd}` needs
  it); the assignment refusal is what keeps a restore from reading it.
- **A resurrected row's `restoreDelivery` is `Restore::label`, and
  `restore_step` is the only place that chooses it.** The wait and the send
  are injected into `restore_step`, so its order (nothing waited on or sent for
  an unregistered, refused or empty restore; nothing sent to a target that is
  not ready) is a table test:
  `a_restore_step_is_labelled_and_runs_its_effects_only_when_due`. A new label
  is a new `Restore` variant with its row there and its entry in CONTRACTS.md's
  `resurrect` paragraph; don't write the string at the call site.
- **The terminal candidate arm is gated on `entry.restore.is_some()`, never
  on `agent == "shell"` (P-C6).** `resolve_candidate` tries the harness arm
  first (`AgentProfile.resume_args`, unchanged) and only falls to the
  terminal arm — `[<login shell>, "-l"]` via `login_shell` — when the
  harness arm found nothing AND a `restore` snapshot is present. A
  `restore`-less shell entry (predating P-C5) still hits the pre-existing
  taught skip; don't widen the gate to bare `agent == "shell"`, which would
  resolve a candidate this crate has no captured facts about.
- **A `ByLog` harness (eidolon) never reaches the terminal arm, and its
  journal is `journal_of`'s alone (ruling 161).** `resolve_candidate` matches
  `ResumeArgs::ByLog` before the terminal arm, and `journal_of` refuses every
  `logPath` that cannot name a journal: absent, not absolute or holding an
  unsafe character, inside `session_logs_dir()` (a terminal transcript), a
  symbolic link (its own sentence, read with `symlink_metadata`), or not a
  regular file. The refusal is the candidate's skip reason, the same sentence
  the launch and the dry run show. Don't give it a terminal-arm fallback, and
  don't read a `logPath` under `session_logs_dir()` as a journal.
- **One engine per session is decided before the spawn, under the stage
  lock (`claim_resume`).** The claim is the only write a launch makes before
  its spawn, and it runs before `launch_session`. The presence scan is read
  outside the lock, the roster and the claims inside it, and no spawn runs
  under it, because the new session registers under the same lock. Don't move
  the claim after the spawn: a second process would find none. A claim's
  verdict is its new id's record and its age, never a pid. A by-journal launch's
  claim also names its journal (`Claim.journal`), and the block refuses a
  candidate for any in-flight claim that names its session or its journal,
  whichever session holds it (`Blocked::InFlight.by_journal` says which). A
  claim stamped more than 5 seconds ahead of the clock is stale and blocks
  nothing; within that skew its age reads as 0. A clock step of more than 5
  seconds backward, or more than 60 seconds forward, during a launch's
  registration is the residual: it can let a second launch through. The block
  itself reads no file; `Journal::of` resolves each journal once before it.
  `release_resume_claim` drops a claim only while it still names its new id.
  The gate sits in `resurrect_one`, so every mode that reaches it (bare,
  `--all`, `--id`, a held replay) passes it. Manifest clean spawns take no
  claim, since they are not ledger entries.
- **A test that re-drives one ledger entry must end its first revival
  first.** A claim is in flight for up to 60 seconds while no session holds
  its id, and a fake terminal never registers one. Unless the first revival
  is marked `done` in `sessions.json`, the second launch is refused as in
  flight. `transfer_is_idempotent_when_the_pair_has_already_transferred` does
  this.
- **`reap`'s one carve-out into the shell kind gate is `restore.is_some()` +
  `spawned` + `state == "idle"`, never `pid`-liveness alone
  (`abandoned_spawned_shells`, `reap.rs`).** The kind gate every other
  signal in `is_session_dead` respects exists because "a shell record's pid
  IS its terminal" — a live pid must never be overruled by staleness. That
  holds for a shell the operator opened, where a live pid means a human is
  (or may be) sitting at the window. It does not hold for a worker shell
  `aoide spawn` left running behind an agent: there the
  ONLY way anything ever reaches it again is the injection door
  (`aoide send`), which cannot tell an agent's own follow-up from a
  human's (`send.rs`'s own `resolve_sender` doc — attribution is
  self-reported and unenforced, never a trust boundary). So the touch
  signal is the per-session pty LOG FILE's mtime (`log_path`), not
  attribution: ANY byte crossing the pty — the spawned command's own
  output, or any later send from anyone — resets the clock, which is what
  keeps a human's later use of an agent-spawned terminal safe from this
  signal without ever needing to know who touched it. `restore.is_some()`
  (stamped only by the P-C5 tick, itself gated on
  `program_is_a_shell`) keeps this from ever reaching a headless
  AGENT or a one-shot command — neither ever populates `restore`, so
  `log_mtime` structurally has nothing to read for them and the signal
  never fires. **A shell has no self-heal, unlike an agent** (whose hook
  door re-registers a falsely-reaped record on its next event) — this is
  why the carve-out stays this narrow, and why widening it needs the same
  bar this bullet documents, not a looser one.
- **Every conduct-owned pty tees to `state/sessions/<id>.log` (task #15,
  "everything tees" — no opt-out flag), through the ONE `open_session_log`
  open+stamp path `graph/conduct.rs::session_conduct` calls for both arms.**
  Three properties stay load-bearing if this is ever touched: (1) it tees
  the MASTER-READ side only — what the pty emits — never raw stdin, so a
  no-echo `sudo` password prompt never lands in the log; (2) a log write
  failure DEGRADES (`OutputSink::write_log` just stops mirroring) and must
  never block or kill the interactive pump, which is raw-mode and
  latency-sensitive; (3) the directory (`0700`) and file (`0600`) modes are
  set explicitly via `OpenOptionsExt`/`PermissionsExt`, never left to
  umask. Don't reintroduce a second open/stamp call site for either arm —
  headless and interactive both go through the one function.
- **A wrapped program with no hook and no trace says it is waiting on a human
  through the pty bytes the wrap already relays, in `graph/prompt.rs`, and
  nowhere else.** `conduct_multiplex` feeds the master-read side to
  `PromptWatch::output(bytes, now)` and the stdin or inbox bytes to
  `input(bytes)`, which classifies them (a printable key or Enter answers the
  row; arrows, Tab, Ctrl keys and other escapes do not) and keeps none of them.
  Its ~1s tick asks `standing(now)`: the last non-empty CR/LF line of the recent
  output (`strip_ansi` over the last `WINDOW_BYTES`, a CHA to column 1 read as
  a CR) has stood unchanged for `QUIET` (3s), matches ONE named row of `ROWS`,
  and no key or Enter has answered its row. The episode identity is (text, row),
  where a row is the count of newlines before the line; a repaint keeps both and
  so keeps the clock. The child still running is the loop still looping. Hold
  these when editing:
  - **The table is closed and each row is end-anchored.** A row is a named
    `fn(&str) -> bool` over the lowercased, flattened line and a test beside
    the others — every row's positive and its near-misses (a sentence that
    only mentions `[y/N]` or `password`, a `+  password:` diff line). A new
    prompt is a new row, never a looser one; a false `awaiting` is noise a
    parent is paged for. The `sudo` row is the only row that also raises
    `needsSudo`; a shell's foreground-is-`sudo` check (`sudo_awaiting`) holds
    that badge on its own.
  - **A shell's screen is silenced only for an interactive shell at its bare
    prompt.** `program_is_an_interactive_shell` reads the wrapped argv: no `-c`
    string and no script operand. For such a shell `shell_snapshot` drops the
    prompt when the shell is idle (`interactive && idle`, which for an
    interactive shell is the pgid check alone), so its own `read -p` is not
    heard. A shell given `-c` or a script (`bash -c "apt install x"`,
    `sh ./run.sh`) shares its process group with the command it runs, so the
    foreground pgid cannot tell that command's question from the prompt; its
    questions are heard like any program's. The decision reads argv only; no
    `/proc` probe is added.
  - **Precedence is hook phase > trace > screen > presence.** The wrap is
    watched only when it is a shell or its label has no `agent_profile`
    (`watch_prompts` at the `conduct_multiplex` call). `do_session_refresh`
    re-checks `hooks.json` under the stage lock: `hooked` is computed only for
    non-shells, and a non-shell that gained a hook record has the `question` and
    `needsSudo` the screen set taken back, and sets nothing (the hook door owns
    its `state`). A shell's `state` is its own snapshot whatever records it
    holds. The screen clears `awaiting` only as its own: a state it did not set
    (`working` from a `session phase`) is never rewritten, and `idle` is what an
    answered question returns to. A record that is `done` keeps no question and
    is never set `awaiting`; the reaper's done write clears both fields.
  - **The line is foreign text (house rule 4).** It leaves the module only as
    the record's single `question` field: escape sequences and control
    characters stripped, flattened, `"` softened, cut to `QUESTION_MAX`
    keeping its END. The window of recent output is memory only, bounded at
    `WINDOW_BYTES`, and never written; a screen that matches no row leaves
    nothing behind. The text is held in three places, each capped and
    sanitised: the record's `question`, the ping-back cursor's `screen`
    entry, and the audit line of a ping that carried it. The session log
    still tees the master-read side only.
  - **Pure text over the pty.** No `/proc`, no `cfg(target_os)` — the shell
    tick's foreground facts stay in `conduct_refresh_shell`, the screen is one
    portable path for shell and non-shell alike. The non-shell wrap writes
    through the same `do_session_refresh` (change-only, one stage lock), with
    no shell snapshot.
  - **The notifications' summons card is not raised for it.** `permit::
    spawn_summons` is the hook door's and types a profile's approve/deny
    keystrokes; a program with no profile has none. The conductor, the dock
    and the A2A `input-required` fold read `state: awaiting` and need nothing
    more.
- **Gate the sweep on `spawned`, never on `headless` and never on
  `parentSessionId`.** `headless` is the wrong axis: `spawn --windowed`
  execs a real terminal running the same `aoide conduct`, and a windowed
  worker terminal is abandoned exactly as readily as a headless one.
  `parentSessionId` is worse than wrong — `graph/doc.rs` clears a child's
  parent edge when the parent leaves the roster (a protected live wrap keeps
  it, CONTRACTS rule 3), so it is empty at
  precisely the moment a shell becomes leftover, and a predicate keyed on
  it would match only shells whose agent is still alive. `spawned` is
  stamped once by `stamp_spawned` inside the child `spawn` re-execs and
  never cleared, and `aoide spawn` is its only writer — which is what makes
  "an agent left this, the operator did not" decidable at all.
- **The abandoned-shell band is `REAP_SPAWNED_SHELL_STALE_SECS` (2 days) and
  stays its own constant.** Don't fold it back into `REAP_IDLE_STALE_SECS`
  to save a line: that band guards a session that might still be someone's
  and has to clear a weekend, while this one judges a terminal an agent
  created and walked away from, where being early costs an untracked shell
  that keeps running (no signal in `reap.rs` kills a process) and being late
  costs a roster full of worker terminals. The two move for different
  reasons and will keep diverging.
- **A human gesture waives that band, and is resolved AT THE DOOR
  (`with_human_gesture`), never inside the sweep.** `reap_and_announce`
  normalizes it onto the invocation as `--now` BEFORE `daemon_dispatch`,
  because a resident `aoided` runs the sweep over there with no tty and a
  `Door::Daemon` invocation: a `pick::interactive` probe made on the far
  side answers false for every gesture, and the dock button's behaviour
  would then depend on whether the daemon happened to be up. **The flag is
  the fact; the probe is only how the CLI door computes it** — a new caller
  that means the gesture passes `--now` itself (`shellbridge.rs`'s
  `dispatch_recheck_sessions` does, since a detached child has no tty to
  probe). Waiving drops ONLY the staleness clause: `spawned` +
  `restore.is_some()` + `state == "idle"` + `!exempt` all still hold, and no
  other band in the pass moves.
- **`exempt` (task #20) is filtered out of candidacy BEFORE the band
  question, in both `is_session_dead`'s third signal and
  `abandoned_spawned_shells`, so it survives `--now` structurally rather
  than as a special case inside either.** Don't implement it as "skip this
  id in the sweep loop" bolted on after the fact — the veto belongs inside
  the SAME predicates every other guard in this file lives in
  (`spawned_shell_shape` is the one shared shape both
  `abandoned_spawned_shells` and `spared_exempt_spawned_shells` read, so a
  record can never be both reaped and reported spared, or neither). It
  vetoes staleness ONLY: pid-gone, pre-boot ghosts, superseded
  duplicates, orphaned subagents, and done-sibling tombstones all still fire
  on an exempt record, because those are positive-evidence signals, not
  staleness — the exemption's whole promise is "never take a LIVE session,"
  not "never take this id."
- **`--announce` is not a second spelling of `--now`.** It means "toast even
  on a quiet pass" — a display concern — and welding the two together would
  leave a caller that wants the answer without the sweeping-now no way to
  say so. Both flags happen to ride the dock button; that is the button's
  choice, not an implication.
- **The nothing-to-restore warning fires at MARK time, not at resurrect
  time (task #100).** `undying.rs::nothing_to_restore_warning(agent,
  has_capture)` mirrors `resolve_candidate`'s own two arms — a registered
  harness `AgentProfile.resume_args`, or `has_capture` (this session's own
  P-C5 restore snapshot, gated by `program_is_a_shell` above) — and both
  mark sites (`spawn --undying`, `session grant undying on --id <id>`) append its
  text onto their own Outcome MESSAGE, never a log line, whenever NEITHER
  arm would resolve. `spawn --undying` computes `has_capture` directly off
  the command it just built (no roster read-back, no race against the
  conducted child's own first refresh tick); `session grant undying on` reads it
  off the LIVE roster record's `restore` field and stays silent for an id
  absent from the roster (no live signal to warn from — same posture `live`
  already takes) and for `off` (a future restore isn't promised either way,
  so there is nothing to warn about). Don't move this check to resurrect
  time "to simplify" — the whole point is the operator finds out BEFORE the
  undying set is relied on, not after a later resurrect silently restores
  nothing.
- **`hookAncestry` is the chain of the latest registration or heal, starts at
  the hook's harness, and moves only where `session_store::ancestry_may_move` allows.**
  `window::hook_chain` walks up from the hook's parent past every process whose
  argv[0] basename is a shell or a forwarding launcher (`conduct`'s
  `is_invocation_plumbing`, reading the lists `SHELL_BASENAMES` and
  `FORWARDING_LAUNCHERS` in `conduct.rs`) and takes the first other process as the
  harness. The walk reads process ancestry only, so a process that owns a window
  is an ordinary process to it. An unreadable pid counts as plumbing, and no
  harness above the hook means the door moves no chain. The chain lands in one
  locked write on three events: the registration (`do_hook_session_start`, except
  on a protected live wrap, whose start writes nothing), a per-turn hook that finds
  the record chainless (`ensure_session_chain`, through
  `session_store::chain_is_takeable`), and a per-turn hook that heals a stale chain
  (the same call). Registration is a SessionStart, or the first hook event of an
  unknown id, so a resumed harness that runs in a new process gets a current chain.
  `ancestry_may_move` refuses only while the record's session is live and its own
  pid or its harness (`harness_pid`: the first stored pid, else the pid) is alive
  outside the new chain, on the registration and the fill alike, so two live
  processes under one session id do not trade the chain: the holder keeps it until
  it exits. Those two processes are the only anchors, because a chain-wide liveness
  test would hold a stale chain for as long as any process of it lives (a shell
  outlives its harness). The chain is the hook door's kernel fact, never a payload
  claim, and attestation never reads it (`attested_record` reads only sealed pids).
  The fill and heal are stricter than registration. A chainless record takes the
  chain unless a live process of its own holds it outside the hook's chain
  (`lives_apart`). A stored chain is healed only on a dead-to-live transition: the
  record holds no live pid of its own outside the hook's chain, and the hook's chain
  holds a live harness the stored one lacks (`chain_is_stale`). Both chains start at
  the harness, so no shell layer is counted: the walk reads the same way under a
  direct child, a compound `sh -c`, an exec-in-place `sh -c` and a capture pipeline
  (`window.rs`'s `a_direct_child_hook_finds_its_harness_at_once`,
  `a_hook_under_a_compound_sh_c_finds_its_harness_past_the_shell`,
  `a_hook_under_an_exec_in_place_sh_c_finds_its_harness` and
  `a_capture_pipeline_hook_finds_its_harness_past_the_pipeline_shell` pin the four).
  A bound turn whose stored chain is live is left alone: one read of `sessions.json`
  and one ancestry walk, and no compositor is asked anything
  (`a_bound_turn_with_a_live_stored_chain_is_left_alone`).
  Two residuals. A hand-run hook under an `aoide conduct` wrap or a multiplexer server
  takes that process as its harness, since the plumbing list names shells and
  forwarding launchers only. A hook behind `timeout`, `script`, `nix`, `su`, `doas` or
  `sudo` takes that launcher as its harness, and its stored chain heals once per turn,
  since each turn's launcher is a new pid (`program_is_a_shell` reads those names, the
  plumbing list does not). Making the wrap or the server plumbing is a new entry in
  `conduct.rs`'s lists, which changes the verdict every reader of
  `is_invocation_plumbing` gets. Adding `timeout` to `FORWARDING_LAUNCHERS` is not a
  one-line change: its duration is positional, and `program_is_a_shell`'s `timeout`
  arm exists so a bare number is not read as an option. The shared far ancestors
  and systemd stay alive while any sibling runs, so their liveness is not
  evidence of the record's own process. Its consequence: a live stored chain is
  never rewritten by a per-turn hook, and no write happens per turn. Two readers
  consume it. A later `wrap`/`conduct`/`spawn`
  registration with no explicit `--parent` reads it via
  `window::ancestry_parent`/`resolve_registration_parent` to find its true
  launching agent by intersecting ITS OWN `/proc` ancestry against every live
  agent's `hookAncestry` (deepest/closest match wins), falling back to the
  ambient `AOIDE_SESSION_ID` env only when nothing intersects. Precedence is
  explicit `--parent` > ancestry walk > env, in that order, for all three of
  `wrap`/`conduct`/`spawn` (spawn re-execs `conduct --headless`, so fixing
  `conduct`'s own call covers it). The window sweep reads it to bind a
  pid-less hosted session (`window::window_for_hook_chain`), and binds while any live process of that chain, a tab shell included, stands beneath the window's owner. Neither
  reader ever turns the chain into a pid.
- **`parentSessionId` on a hook record is re-stamped from ATTESTED kernel
  process evidence at SessionStart and on every per-turn hook, not only at
  registration — unlike `hookAncestry` above, which moves only where
  `ancestry_may_move` allows, and heals on a per-turn hook only through
  `chain_is_stale`.**
  `send.rs::hook_ensure_session` walks the hook-firing process's real
  `/proc` ancestry (`identity::attested_wrap`, narrowed to a sealed,
  `conductable` ancestor) on the `Phase` arm's registration self-heal and
  `PhaseIfRunning` — each fires once per turn, BEFORE the exists/fresh
  split, so an EXISTING record gets this too, not only a freshly-registered
  one — and re-stamps `parentSessionId` via
  `session_store::stamp_attested_parent` whenever the resolved wrap differs
  from what's currently stored, unless it is a conducted wrap (change-only,
  cycle-guarded by
  `would_cycle`). `ToolStart`/`ToolEnd`/`SubRekey`/`SubEnsure` —
  PreToolUse/PostToolUse and their subagent siblings, fired on every tool
  call — skip the walk outright (`attest: None`): a terminal's wrap only
  ever changes across a process restart or a `--resume`, never
  mid-tool-call. `HookAction::Start` — SessionStart itself, in
  `hook_for_profile_gated` — resolves the SAME attested walk directly
  through the pure resolver `send.rs::start_parent`
  (`attested.or(env_parent)`), and the resolved parent feeds straight into
  `do_session_start`'s own upsert (`aoide_storage::session::upsert_session`
  re-stamps `parentSessionId` on an EXISTING id whenever `parent` is
  `Some`; `do_session_start_inner`'s cycle guard covers it, no second
  guard). This is what makes a harness id that survives `--resume` (the
  same session id, hosted by a DIFFERENT terminal after the resume) follow
  its CURRENT wrap from the very first hook after the resume. Never a
  guess from cwd/title/workspace; no daemon reachable, or no conducted
  ancestor found, leaves the field untouched — fail-closed. A conducted wrap's
  parent is never written this way (`conducted_wrap`). The ordinary
  `AOIDE_SESSION_ID` env fallback still exists on both paths, but only
  behind the attested wrap. `hook_ensure_session_with` is the injection
  seam (the `deliver_local_with` precedent) for the per-turn path:
  production wires `real_attested_wrap`, tests inject a resolver — the
  composed hook→re-stamp path is proven in-crate through it, never against
  a live daemon; `start_parent` takes already-resolved values, so the Start
  arm's preference order is a plain unit test.
- **`who` and bare `session`'s old undying-picker meaning are BOTH retired
  (session-surface redesign, command-defrag lane X, 2026-08-28 — hard
  cutover, no alias).** Bare `session` is now the ROSTER (`who.rs`'s
  `session_roster`/`session_roster_with` — grouped by PROJECT bare, by HOST
  under `--hosts`), and the undying picker/mark live at `session grant
  undying` (`grant.rs`'s `session_grant`, dispatching on a positional
  `<kind>`). Don't reintroduce a standalone `who` registration or a bare
  `session` picker "for compatibility" — both spellings are unknown now,
  same as a typo; the mechanisms they backed (the roster's probe pipeline,
  the picker's row-building) survive unchanged, only the registered paths
  moved.
- **`session grant` has exactly one scripted spelling for the undying
  mark — `session grant undying on|off --id <id>` (U1, relocated).** Don't
  add a `--undying` flag or any other scripted shortcut anywhere else — a
  non-interactive reach to the picker branch (non-CLI door, no tty,
  `--json`) is ALWAYS steered to that one existing spelling
  (`grant.rs::require_cli_tty`), never given a second one of its own. Don't
  gate the picker on `Door::Cli` alone either — `pick::interactive` also
  requires a real stdin/stdout tty, and `--json` must refuse even ON a real
  tty (a picker's prompts have no business interleaving with a
  machine-readable stream a caller explicitly asked for). The scripted
  branch (`undying.rs::undying_grant`) carries NO such gate — unchanged
  from `session undying`'s own reach from any door, since a script/daemon
  needs it too.
- **`grant.rs`'s picker branch never live-probes a node.** Its node rows
  come from `node_store::load_node_cache` fed through `who.rs`'s
  `sessions_from_graph` (`pub(super)`, `SessionView` alongside it, U3). Don't
  route it through `who.rs::probe_nodes`/`collect_roster`'s live-pull path
  "for freshness" — the brief this landed under is explicit that the
  picker's own tty round-trip must never block on network I/O, and bare
  `session`/`--hosts` already own the live-presence job.
- **A node row's mark writes a `.aoide/project.json` spec, never
  `state/undying.json` (U3) — the id lives on the node, this conductor
  cannot write ITS store.** `grant.rs::apply_diff` resolves "current
  project" via `aoide_storage::manifest::walk_up` from cwd, the identical
  discovery `resurrect`'s own bare-manifest mode uses (U2); no manifest
  found there is NEVER a reason to create one — every node
  mark/unmark in that confirm reports `skipped[]` with a taught reason
  instead, while any LOCAL rows in the SAME confirm still apply normally.
  Don't fold the "no manifest" case into a hard command failure — a picker
  confirm can genuinely be half-local, half-node, and the local half's
  success must never be held hostage to the node half's missing manifest.
- **A node row's cwd that `node_spec_dir` cannot relativize under the
  project root has NO savable fallback — it is REJECTED before it ever
  reaches `manifest.sessions`, never the raw absolute cwd (U3).** `save_manifest` refuses the WHOLE batch if ANY spec carries an
  absolute `dir`; a raw-cwd fallback here would therefore not merely write
  an inferior spec, it would silently fail the save for every OTHER
  legitimate node change queued in the SAME confirm while `changed[]` still
  reported them all as persisted (the exact defect this bullet exists to
  prevent). Don't reintroduce a `(dir, relativized)`-shaped fallback —
  `node_spec_dir` returns `Option<String>`, `None` means "no spec, skip
  with a taught reason," full stop. Correspondingly, `apply_diff`'s
  `changed[]` for the node path is populated ONLY after `save_manifest`
  returns `Ok` — a failed save folds every pending node change for that
  confirm into `skipped[]` instead, never a false `changed` entry for a
  write that never landed. `build_rows`' own node pre-check goes through
  the SAME `node_spec_dir` relativization for the identical reason: a raw
  cwd compared straight against a manifest spec's (always project-relative)
  `dir` can never match, which would silently show an already-undying node
  session as unmarked.
- **Unmarking a node row removes EVERY spec matching `{host, dir, agent}`,
  not just the first (`Vec::retain`, never a single `Vec::remove` by
  position).** A hand-duplicated entry in `.aoide/project.json` (an
  operator who edited the file directly) is cleaned up in one unmark, not
  one confirm per copy — don't narrow this back to a first-match removal.
- **The roster (`who.rs`, reached at bare `session`/`--hosts`) is a
  projection, never a store.** It must never write
  `state/node-cache/<name>.json` — `build_graph`'s own fold (`doc.rs`) is
  the ONLY writer of that cache. Its live probe reads straight off the
  network via `aoide_client::commands::pull_node_live` and falls back to
  the cache (read-only) for an unreachable node; don't "helpfully" have a
  successful live probe refresh the cache as a side effect.
- **`session --hosts`'s and bare `session`'s PROJECT grouping share ONE
  `collect_roster` — never two collection passes.** `who.rs`'s `Roster`
  struct (`host`, `projects`, `locals`, `nodes`) is built ONCE per
  invocation; `session_roster_with` branches ONLY on how it renders/groups
  `nodes` afterward (`render_nodes`/`node_json` for `--hosts`,
  `group_by_project`/`render_groups`/`group_json` otherwise). Don't
  duplicate the local-stage-load + node-probe sequence for a future
  grouping — extend the branch, not the collection.
- **`project_bucket` reuses whichever project attribution the codebase
  already computes — never a third one, and only for a LOCAL session.** A
  registered `projects.json` name via `anchor_for` (pure string matching)
  wins; else a `.aoide/project.json` manifest found by walking up FROM THE
  SESSION'S OWN CWD on this host's own filesystem
  (`aoide_storage::manifest::walk_up`) renders by that directory's basename.
  Don't walk up from the CURRENT process's cwd instead (that's `grant.rs`'s
  "current project" concept, a different thing) — `project_bucket` must
  attribute EVERY local session by its OWN cwd, or a multi-session listing
  would silently misattribute every session but the first. A session from
  another node never reaches it: `who.rs::remote_project` is its whole rule
  (the next entry).
- **A project is a set of anchor roots — `anchor_for` is longest-root-wins
  across EVERY root of EVERY project, not just each project's first.**
  `cwd_under` itself stays single-root (unchanged) and is simply called
  once per root; only its caller spans the set. `project add NAME PATH…`
  ADDS one or more roots to a name — registering it if new, growing it if
  not — and never replaces what is already there; `--new` is the guard
  against a typo'd name silently joining an existing project instead of
  registering its own. `project edit NAME PATH…` is the ONLY command that
  REPLACES a project's whole root list outright — the FULL given list,
  deduplicated, becomes `roots`, and `path` mirrors `roots[0]`; `roots` is
  never "just the extras" (see `aoide-storage`'s own docs). It never
  touches the name or `autoResume` — `add`/`remove` stay the only ways a
  project appears or disappears. `project remove NAME [PATH]` drops one
  root, promoting the next remaining one into `path` so `path` always
  equals the first root, or with no `PATH` drops the whole project — as do
  the project's workspace bindings, which live on the record. Removing a
  project's LAST root is the one root removal that does NOT delete the
  project: it is left standing with no folder (a NAME-ONLY project).
  `project add` and `project edit` both validate EVERY given path
  (absolute, an existing directory) BEFORE mutating anything — one bad
  path in a multi-path call writes nothing.
- **A project may have NO folder — `project add NAME` with no path
  registers a name-only project, never the cwd.** The cwd is not a
  default root: registering the directory you happen to stand in is how a
  project anchors sessions nobody meant it to. A rootless project has an
  empty `Project::roots()`, so `anchor_for` skips it structurally — its
  per-project root scan finds nothing to match, with no special case and
  no `cfg`; it is reached by a workspace
  binding or an explicit `session project NAME` alone, and a folder is
  added later with `project add NAME ROOT`. `project list` prints its name
  with no path column.
- **A binding is stored on the project, and only one project may hold a
  workspace.** `Project.workspaces` (`aoide-storage::records`, additive,
  `skip_serializing_if` keeps it off the wire when empty — every
  `projects.json` predating it stays byte-identical) is the whole store.
  `workspace set` is the ONLY writer that adds: it MOVES the id off whatever
  project held it, in the SAME `with_stage_lock` hold as the `--new`
  existence check, so two racing calls can never both register a name and no
  path can leave one id in two projects. `binding_for` (`graph/model.rs`) is
  the ONE lookup — the binder, the lister and the resolver's workspace rung
  all read it, so they cannot disagree about what "bound" means. A binding is
  NOT a root: it anchors nothing by cwd (a rootless project may carry one),
  `project remove NAME ROOT` leaves it alone, and only the bare `project
  remove NAME` takes it — with the record it lives on, because that is where
  it lives. `workspace list` merges bindings with the workspaces local
  sessions report and never writes.
- **`project add`/`project edit`/`project remove` are daemon-owned atomic
  mutations (`manage.rs`'s `local_daemon`)** — the same door-gated shape
  as `actions.rs`'s `assign_project`/`session_kill`: a `Door::Cli` caller
  forwards the WHOLE invocation through
  `aoide_client::daemon::daemon_dispatch` to the live `aoided` and turns
  "no daemon answered" into a real error (`aoided must be running for
  project management`), never a silent local write; a `Door::Daemon`
  caller (already inside `aoided`) takes the local path directly —
  `daemon_dispatch` short-circuits to `None` on `Door::Daemon` for exactly
  this reentrancy reason; every other door is refused as local-only.
  `register_bootstrap_project` (also in `manage.rs`) is the one exception:
  `aoide onboard`'s `register_project` is its sole caller, calling straight
  into `add_roots` before any daemon exists to dispatch to, never through
  `local_daemon`. The local
  mutation lives in three inner helpers (`add_roots`/`remove_roots`/
  `edit_roots`), each wrapped ONCE, end to end, in
  `aoide_storage::fs::with_stage_lock` — the `--new` existence check (or
  the membership check, or the replacement) and the write happen under the
  SAME lock hold, so two threads racing one project name can never both
  observe a stale registry and both win. Nothing execs under the lock. A
  handler test that exercises the LOCAL path (no daemon, like every other
  test in this crate) stamps its `Invocation` with `Door::Daemon`, not
  `testutil::invocation()`'s default `Door::Cli` — see the handler tests
  in `graph/manage.rs`.
- **`node list` (`graph/node_list.rs`, task #120 P2) is the roster core's
  probe under a wider fold — never a fork of it.** Its presence/session data
  comes ONLY from `who.rs`'s `pub(super)` seam (`probe_nodes`/
  `build_local_node`/`build_node_node`/`SessionView`/
  `NODE_PROBE_TIMEOUT_SECS`) and its advertising data ONLY from
  `aoide_client::discover::run_sweep` — a second prober, a second presence
  classifier, or a private sweep re-implementation here is the exact
  cross-copy this crate's discipline forbids. It inherits the roster's
  projection rule wholesale (writes nothing: not `state/nodes.json`, not
  `state/node-cache/`), a failed/empty sweep only ANNOTATES the roster
  (never fails the command — the paired half is still true), and both
  network seams (`PullFn`, `SweepFn`) stay injected so its tests never
  open a socket. Row/mark grammar and `--json` shape are CONTRACTS.md
  §7-pinned — a rendering change is a contract edit first.
- **`--mesh` (P-14 M2) never spawns the sweep thread at all — it is not
  merely filtered afterward.** `node_list_with` only calls
  `std::thread::spawn(sweep)` when `--mesh` is absent; under `--mesh` the
  injected `SweepFn` is dropped unexecuted. A test proving this MUST inject
  a non-empty heard-set and assert it never reaches the document — an
  empty-heard-set test would pass just as well on the wrong implementation
  (filtering an always-run sweep's result down to nothing) and prove
  nothing about which one is actually running. `assemble_roster` is called
  with an empty heard slice regardless — the SAME roster-assembly seam
  bare `node list` uses, not a second row builder — so `advertising` reads
  `false` throughout and no candidate row ever appears.
- **`who.rs::cached_presence(presence, fetched_at) -> &'static str` is the
  ONE gate a cached session's presence passes through — don't rewrite a
  cached `SessionView.presence` by hand anywhere else.** `build_mesh_node`'s
  cache arm (the `Err` branch, `Some(entry)` case) is its only call site,
  looping every session `sessions_from_graph` produced off the cache and
  reassigning `presence` through it. The rule: `"done"` always survives
  verbatim — both `node list` and `session`'s `presence != "done"` filter
  depend on that exact string surviving the cache fallback unchanged, so
  don't fold `done` into `last-seen`/`unknown` even when the cache carries
  no `fetchedAt`. Everything else becomes `"last-seen"` when the cache
  entry has a `fetched_at` and `"unknown"` when it doesn't. A node that
  never had a successful pull (`load_node_cache` returns `None`) has no
  cache to rewrite and surfaces no sessions at all — `never-pulled` stays a
  node-level state with an empty session list, not a session-level
  presence value. A LIVE host's own sessions (the `Ok(graph)` arm) never
  pass through this gate — their presence is `session_presence`'s ordinary
  online-host classification, verbatim.
- **`SessionView`'s `title`/`model`/`kind`/`parent`/`task` are additive and
  independently absent — never defaulted, inferred, or resolved across a
  host boundary.** `build_local_node` fills them from `SessionRecord`'s own
  `title`/`model`/`kind`/`task` plus `resolved_parent` (bare session id,
  matching `session_id`, never node-scoped); `sessions_from_graph` fills them
  from a remote node's own graph document — `title`/`model`/`role` (published
  back under this struct's `kind` name)/`task`, and the `spawned` edge whose
  `to` is this session, `from` stripped of its `session:` prefix. A node that
  didn't publish a given key leaves the matching field `None`; nothing
  here fills a gap from another field, another session, or another host.
  `effective_project` stays out of this list on purpose — remote rows
  never carry one (S-D's territory), and `--mesh` does not change that.
- **`SessionView`'s `remote_parent`/`remote_children` are the same two
  additive fields, and they are NEVER resolved into `parent`** (P-RSA S4).
  A local row fills them from `SessionRecord.remote_parent` and from
  `aoide_storage::remote_children::load_remote_children()` (`build_local_node`
  reads that one file itself — a two-field join, never worth widening four
  call sites); a remote row reads the far document's own `remoteParent`/
  `remoteChildren` back VERBATIM, because that node resolved the names against
  ITS registry and a second lookup here would consult a registry the document
  never came from. A link missing its `sessionId`, or naming no `node`, is
  dropped, never rendered blank (`remote_link` — a foreign document is display
  data either way). `parent` stays what it always was — the LOCAL `spawned`
  edge — and
  `resolved_parent` never sees a remote field: that separation is the whole
  point of the second field (the autogate grant and sibling rule in
  `send.rs` read `parentSessionId`), so do not "unify" them.
- **`graph::find` answers from its index alone (SESSION-FIND B3-B5).** The
  matcher reads `Row`s and nothing else: no file, no lock, no node dialled,
  and nothing written. It never infers a session from a name. A `parent:`
  reference resolves through `storage::addr::resolve` with the `send --to`
  rule, case-sensitive, and an ambiguous one is a refusal listing its
  candidates, never the first match; a far session's name typed without its
  node is refused with the `node/name` form (`held_elsewhere`). Only a local
  row's own `remote_parent` names a parent: a far row's is its node's
  unvouched claim, so it never makes a local session its parent
  (`parent_match`), and a far node's claim about a row on another node never
  makes a `--related` reason (`reasons_between`). `mailbox:` and `cc:` read a
  row only when it is local (`mailbox:` by `key_fields`, `cc:` by a `None`
  `cc` on a cached row), so a far node's unpublished mail facts never match.
  Terms filter and only bare words rank; the four sort keys are the order,
  and nothing else reorders a result. `related` ranks by its strongest
  reason, then by how many reasons it has, then by `tiebreak`. `mail_facts`
  takes the peeked cursors and entries as arguments and never reads them
  itself; a cached row gets no mail facts at all, so its mail reasons never
  hold. `load_index` is the only reader, and it reads files only: the stage,
  each node's cache and a mailbase peek, never a lock or a migration.
  `session_find` calls no probe and no pull. Its box name arrives as an
  argument (`session_find_with`), so a test never reads it, and its no-dial
  and no-write proofs are the find tests
  `find_reads_the_stage_and_the_caches_and_writes_nothing_and_dials_no_node`.
- **`graph::find` stays linear in a hostile far node's rows and a paired node's
  letters, and an answer never grows past its rows times a constant.** A far
  node chooses its rows, ids, cwds and edges, one cache may hold 20 MiB, and a
  paired node can deposit letters into local mail, with a subject, a sender,
  recipients and a mailbox name of any length. So no step may cost rows times
  rows, mailboxes times mailboxes, letters times mailboxes or letters times
  every row, and no allocation may grow with a field's length times the query's
  word count. Every string such a node or letter wrote is clipped where it is
  read (`clip`: `LINE_MAX` characters, reading only the field's head):
  `remote_row` for a far row's text, `local_row` for a local row's text, `filed`
  for a letter's subject, sender, recipients and thread, `mail_facts` for a
  mailbox's name; a task stays whole because `remote_task` already bounds it. A
  far session's ids are never clipped (a clip folds two ids into one): `legal_claims`
  refuses a session whose id, parent or remote link is not a
  `valid_claimed_session_id`, and the scope names the node `ids refused`. A
  `matched` value is clipped the same way, whichever row or letter it came
  from, and names its term by its index in `words`. A query takes at most
  `WORDS_MAX` (64) words and `ALTERNATIVES_MAX` (256) alternatives, refused as
  soon as `split_line` passes the word cap. The resolver's candidates are
  gathered once per answer (`Refs`), and each distinct `parent:` value is
  resolved once. `--related` checks each row against a `Target` built once,
  whose remote children and mailboxes are sets. A row's bare fields are read
  only when the query has a bare word, and the cwd is folded once, its
  components slices of that fold, never a string each. `find` returns rows and
  never a `matched`: `Found::explain` builds one, and only the machine block of
  `list_view` calls it, so a human answer has no flag to get wrong. In
  `mail_facts`, unread counts search each mailbox's sorted marks, a letter is
  decoded once however many rows read its mailbox (`filed`), a row visits only
  the letters filed to the mailboxes it reads (an index by mailbox, merged back
  into the base's order), and a row's threads dedupe through a set;
  `reasons_between` pairs threads and mailboxes and dedupes its reasons through
  sets as well. A letter is held once (`Rc`) and each reader's row holds a
  pointer to it. Every list in a row holds at most `LIST_MAX` (20) beside its
  full count: edges, `reads`, `leads`, `cc`, `threads`, a letter's `to`, a
  related row's `reasons`, and the candidates a refusal names (with `and N
  more`). A machine list row lists `ROW_LIST_MAX` (8) of its mailboxes and
  leads, and carries `cc` and `threads` as counts only. `matched` holds one
  entry per query word, and a machine row lists `MATCHED_MAX` (8) of them beside
  `matchedCount`; ranking reads every match. A machine answer lists at most
  `ROWS_MAX` (100) rows in its message and its data alike, plus one `and N
  more` line, beside `total` and `more`; the human form lists them all. The
  serialized machine answer is bounded by 1 MiB, stated in SESSION-FIND.md
  Decision 13 and measured by
  `the_worst_machine_answer_serializes_under_its_stated_bound`. A new list in a
  row takes a cap and a count, and a new string a node or letter wrote takes a
  `clip` where it is read. The letter visit test counts work through `VISITS`,
  not time. The other wall-clock bounds in `find.rs` are an order of magnitude
  above their idle runs, so a loaded builder passes them, and a quadratic mutant
  still fails them. Keep both properties when touching these paths.
- **A remote row's project is the one its node published.**
  `who.rs::remote_project` is the one rule: the stored `project`, clipped to a
  line, and none when the node published none. Bare `session`'s
  `group_by_project` calls it for every row of a non-local node and
  `find`'s `remote_row` for every cached row; neither calls `project_of` or
  `project_bucket` with a far cwd. A far cwd is a path the node chose:
  bucketing it would stat this box's disk up the whole path and match this
  box's `projects.json` roots, so a node could steer reads here and put its
  rows under a local project. A local row still buckets through `project_of`.
  A node that published no project is under no project in both commands.
- **Row JSON is for a machine reader only.** `sessions` and `related`, with each
  row's `matched`, are built when `--json` is passed or the door is not the CLI
  (`teach::quiet`'s rule in aoide-client), so an MCP caller gets them without
  the flag and a human answer builds none. MCP and A2A carry the registry's one
  `query` argument as one string, so a lone argument there is a query line and
  `words` splits it with `split_line`; a CLI argument and a daemon array
  element are words already and are never split. Every human line
  passes through `fit` (clipped to `ROW_MAX`, 240 characters) after each field's
  `clean_line`, and a fix that names text a far node wrote passes it through
  `plain_word`, which hands the operator a placeholder for anything but one
  plain word. A new human line or a new fix goes through them.
- **Every string `who.rs` prints off a pulled document goes through
  `common::clean_line`** (`shown`/`endpoint` — control characters and the bidi
  formatting marks stripped, flattened, clipped to `common::LINE_MAX`), in the
  two renderings AND in the shared `session_view_json`: the conductor paints
  that JSON, so the terminal is not the only consumer. Nothing off a far node
  is printed raw, and no field reaches a terminal unbounded — a 1 MB `cwd`
  costs one clipped line. If you add a rendered field, it goes through them.
- **`attach_subtrees` is the one place the remote `(node, sessionId)` link is
  joined to the node fold**, and it runs once in `collect_roster` — after every
  node is probed, before any filter or grouping, so both renderings and the JSON
  read ONE answer. It fills `RemoteChild`'s `fold` (`Fresh`/`Stale`/`Absent`)
  and `subtree`; a row the fold does not carry stays visible as `(not pulled)`,
  because hiding a link this box still knows about is a lie of omission, and a
  row whose cache is past `NODE_CACHE_TTL_SECS` stays visible as `(stale)`. Do
  not re-derive the nesting in a renderer — `subtree_lines` reads the joined
  rows, and a second walk would drift. The conductor TUI's `graphview.rs` reads
  neither key yet; teaching it means rendering THIS join, not a second one.
- **No walk over a far graph recurses per level or costs more than linear in
  its sessions plus links.** A pulled document is up to 20 MiB and every
  session id, `spawned` edge, `remoteChildren` and `remoteParent` in it is the
  far node's choice. Its depth rides id links, not JSON nesting, so the parse
  limit does not bound it: a chain of a hundred thousand sessions parses, and
  `aoide session`, `session find`, `graph view` and the conductor all render
  it (the daemon door serves them, so an overflow or a hang there is the
  daemon's). Hold four things. (1) Walk on an explicit stack, and build rows
  flat (`SubtreeRow { session_id, depth }`) so derived `Clone`, `Drop`,
  `Debug` and `PartialEq` do not recurse either. (2) Index once and probe:
  `spawned_into` for the `spawned` edges, `FoldIndex` for parent to children;
  never `iter().find`/`any`/`contains` over a far list inside a loop over a
  far list. (3) Claim per linking node: `FoldIndex::drawn` is keyed by
  (linking node, session), so one node's links draw each of a fold's sessions
  at most once however many of them point in, and another node's link to the
  same session keeps its rows. A far node's link never draws this box's own
  sessions: `attach_subtrees` leaves its rows empty. (4) Bound what is drawn,
  never what is kept: indentation clips at `INDENT_LEVELS` and the JSON nests
  to the same depth, but no session is dropped. A far string that reaches the
  filesystem is bounded too: `project_bucket` starts `walk_up` at the deepest
  ancestor of a cwd that fits `WALK_PATH_MAX` (`within_path_max`), a
  directory whose manifest still stats. The proofs are `a_200k_deep_far_chain_*`
  on a 256 KiB stack (`aoide_test_support::on_small_stack`), run with the env
  lock taken on the test thread, and an absolute `finishes_within` ceiling
  beside each site. A ceiling is not a linearity proof: its baseline runs in
  milliseconds, so a ratio never governs and none is claimed. A new walk over
  a far document gets both.
- **Residual: a cached far document nests inside the local one.** A fresh far
  node's cached graph is copied verbatim into the local document (`doc.rs`),
  and `graphSummary` serves that copy, so each relay hop adds a few levels of
  nesting. A document already near the 128-level parse limit fails the next
  hop's parse (`unparseable response`), which marks that node unreachable for
  its downstream peers. It is a refusal, never a stack overflow. Capping the
  embed is the fix if it comes to matter.
- **A ledger row's doom belongs to the caller that lands the roster.** The
  retain is `doc.rs::drop_remote_child_rows`, called by BOTH roster-exit paths
  (`reap_inner`, `session prune`) AFTER their `sessions.json` write succeeds —
  never inside `drop_sessions`/`prune_done_scoped`, and never before the write:
  the ledger is a projection of the roster, so a failed stage write must leave
  it untouched rather than a live parent whose rows are already gone.
- **`mesh_path()` (`node_list.rs`) names ONE file, `state/stage/mesh.json`,
  and only `--mesh` may write it.** It resolves through
  `aoide_storage::fs::conducting_stage_dir()`, the same root
  `graph::pending_path`/`notifications::notifications_path` resolve through — don't hang
  a second path helper or a raw `state/stage/mesh.json` literal anywhere
  else. `mesh_document`'s write is a single `aoide_storage::stage::
  write_stage` call (atomic write-temp-then-rename), and it is the ONLY
  write `node_list_with` performs — bare `node list` (no `--mesh`) returns
  through the pre-existing `row_json`-built `Outcome` and touches no file
  at all; don't let a future change to that branch start writing here "for
  consistency," and don't let `--mesh` grow a second file. `mesh_row_json`/
  `mesh_session_json` are `--mesh`-only projections built from the SAME
  `Row`/`SessionView` bare `node list` already assembled — never a rendering
  change to `row_json` itself, which stays byte-shape-identical to before
  M2 (a golden/shape change to bare `node list` is a contract edit, same as
  the bullet above). `liveSessions` counts `online`/`stale` sessions only
  (`done` is already excluded upstream by the shared `presence != "done"`
  retain); `cachedSessions` is every session that isn't live — with `done`
  already gone, that's exactly `last-seen`/`unknown`. Wiring a periodic
  refresh timer over this file is out of scope here (a later phase's job);
  this command only ever writes it as a side effect of being invoked with
  `--mesh`.
- **`send::deliver_local`'s success path is the ONE mailbase-filing call reached
  through `session_send` — never add a second one on that path.** Every consumer
  that delivers into an ALREADY-REGISTERED session's socket (`send --id`,
  `--to` resolving local, `pending approve`'s re-drive) reaches it through
  `session_send`, and that is where the receipt is filed. A held network send is
  filed when a local approver releases it, through that same path. A network
  door never reaches it: `aoide-server`'s A2A `do_inject` parks its send in
  `pending.json` (`Door::can_approve`), and its A2A spawn holds the opening turn
  the same way, so neither files anything. `aoide-server`'s `mail_deposit`
  (`a2a.rs`) calls `aoide_storage::mail::deposit` directly for a letter/receipt
  arriving over the wire FROM a peer node. It never touches
  `session_send`/`deliver_local` (there is no local session on either end of
  that path), so it does not widen this bullet's "exactly one" rule — that rule
  is scoped to `session_send`'s own callers, and wire-deposited mail was never
  one of them.
- **Readiness is per-harness and two-valued (`Readiness`); a typed first turn
  waits on `wait_ready`, and an unready target is never typed at.** `spawn
  --prompt` and `resurrect`'s restore pass both wait on it, bounded by
  `READY_BUDGET`, before their send, and report `not-ready` if it never comes.
  `Hook` reads THIS launch's harness `SessionStart` (its `sessionStartAt` stamp
  on a child record of the wrapper, at or after the launch instant; a leftover
  record from an earlier run under a reused id is not readiness).
  `OutputSettled` claims no fact at all: it types once output settles and
  reports the delivery `delivered-unverified`. A third value, a prompt-pattern
  marker, was tried and withdrawn as unanchorable: a banner containing the label
  bought a false `Verified` and an early inject. An A2A spawn does not wait at
  all. Its opening turn is handed straight to `hold_pending`, so nothing is
  typed there.
- **The doorbell's `.ring.lock` is never `.stage.lock`, and it is the
  ONLY lock this crate ever holds across real socket I/O** (P-M5a-2,
  `graph::doorbell`). `with_ring_lock` and `try_stage_lock` are separate
  files for exactly that reason — the stage lock's every other holder
  only ever does a brief in-memory read-modify-write, and a ring's own
  socket connect, write, and submit-keystroke delay must never make one
  of those wait. Don't fold them into one lock, and don't add a second
  socket-holding critical section under `.stage.lock`. It is the
  daemon's own serializer for concurrent rings, not a second policy
  boundary — see the next bullet for the actual boundary.
- **A ring executes only under `Door::Daemon`** (P-M5a-2c, the
  architecture owner's ruling on b8af466): the resident daemon is the
  policy and audit boundary for every ring, not `.ring.lock`. `ring`
  itself has exactly two callers — `mail_ring`'s own `Door::Daemon` arm,
  and the Stop-hook replay when that hook is likewise being handled
  under `Door::Daemon` — and every other door forwards through
  `aoide_client::daemon::daemon_dispatch` instead of calling it. Do not
  add a third caller of `ring` outside those two; a door that needs to
  ring forwards, it never links around the daemon.
- **A ring only ever stamps its latch (`stamp_rung`) AFTER a socket write
  that returned `Ok`, never before and never on a write failure.** The
  reader stays armed on any failure — unreachable socket, dead process,
  a transient write error — so a target that misses one nudge is still
  caught by the next trigger. Do not reorder the stamp ahead of the
  write, and do not stamp inside a `Result`-discarding path that can't
  tell success from failure.
- **Every ring socket write is bounded by `RING_WRITE_TIMEOUT` (2s,
  P-M5c-4) — a wedged peer must never hold `.ring.lock` forever.** Both
  transports connect through `connect_for_ring`, which arms the timeout
  on the stream before either transport's first write, so it covers
  `write_delivery`'s SECOND write (the submit keystroke) as well as the
  first — never re-derive a per-call timeout instead. A timed-out write
  is an ordinary `Err`, so it lands in the SAME `write-failed` skip the
  stamp-ordering bullet above already covers: no new outcome string, the
  latch left untouched, the reader still armed for the next trigger. A
  new ring transport connects through `connect_for_ring`, never a bare
  `UnixStream::connect`, so it can never regress back to an unbounded
  write under this lock — pinned by
  `a_ring_write_to_a_peer_that_never_reads_gives_up_instead_of_holding_the_ring_lock`.
- **A ring never PTY-injects into a non-headless (interactive) wrap, full
  stop — there is no override flag, no `--force`.** Transport selection
  runs first: a live Claude Code channel socket (`channel_socket_path`) is
  a one-way MCP push, never a keystroke, so it carries no half-typed-
  composer risk and is offered to interactive and headless wraps alike.
  Only the PTY path — raw keystroke injection — keeps the interactive ban:
  an interactive wrap with no channel is skipped `interactive-composer`,
  with no flag to force it. Don't lift the PTY-side ban without a
  control-layer guard for that raw-keystroke case (P-M5a-3 in
  `docs/architecture/MAIL.md`'s roadmap), and update MAIL.md's doorbell
  section in the same commit that does.
- **A ring never PTY-injects into a wrap whose WRAPPED program is a shell
  either — `shell-parent`, the shell arm the ping-back lane shares
  (`conduct.rs::wrapped_program_is_a_shell`, never the `agent` label: a
  wrap labelled from a harness profile can be conducting `bash`). Unlike the
  ping-back, this lane has no positive proof of the harness: a wrap the walk
  cannot name is not refused here.** The
  line is submitted, so a shell would RUN it. The channel arm is exempt by
  construction — a one-way push into the harness's own MCP subprocess types
  nothing into any pty — so a shell wrap hosting a channel-connected agent
  is still rung, which is the ordinary terminal wrap's own shape
  (`kitty.nix` conducts the login shell). Refusing the whole record instead
  would silently kill the doorbell for every terminal a human runs an agent
  in.
- **A ring's readiness signal is the CHILD's hook state, checked with
  `aoide_protocol::agents::agent_profile` (returns `None` for an
  unrecognized harness) — never `profile_for_agent` (its `CLAUDE_PROFILE`
  fallback always succeeds and would hide the "never hooked" signal a
  ring needs).** And the keystroke a ring submits with is the CHILD's own
  harness key, never the wrap's — a `kimi` child under a `claude` wrap
  submits with `\r`, not `\n`.
- **The Stop-hook ring replay (`graph/send.rs`) is best-effort and never
  changes the hook's own outcome.** It runs after the hook's real work is
  already decided, discards its `Result`, and never turns a successful
  Stop hook into a reported failure just because a deferred ring's socket
  write failed.
- **The Stop-hook ring replay never rings on the no-daemon local path**
  (P-M5a-2c): `session_hook` gates the replay on `inv.door ==
  Door::Daemon`, threaded down as a single `may_ring` boolean, never a
  global, thread-local, or env var. When the hook cannot reach a daemon
  and falls back to handling itself locally, the replay is skipped
  outright — not attempted and swallowed — and every name still armed
  for that reader stays armed for the next daemon-handled trigger. The
  hook's own `Outcome` is identical either way; only the replay is
  gated.
- **The petname fallback is gated on LIVENESS, not a raw enrolment
  count.** `ring_locked` loads the session roster once and runs the
  petname fallback only when no key in `ring_targets`'s `enrolled` roster
  resolves to a session record that `is_conductable_now` — a latched but
  still-live reader blocks it, but a stale enrolment (no session record,
  or one whose control socket is gone) never wedges a mailbox shut. The
  target walk is unaffected: a dead armed reader still walks and still
  reports `unknown`/`not-conductable`; liveness decides only whether the
  fallback runs. `storage::mail::RingTargets.enrolled` is the reader keys
  (`Vec<String>`, count = `.len()`) so this crate can judge liveness;
  storage stays a pure read with no liveness argument.
- **`doorbell.rs` changes update `docs/architecture/MAIL.md`'s "Delivery
  and the doorbell" section in the same commit** (house rule 8) — that
  section is the design's canonical prose statement; this file states
  only the invariants an editor must hold while changing the code.
- **`graph/spawn.rs`'s `build_conduct_args` is the ONE place the `aoide
  conduct -- <agent cmd>` argv gets built (P-D7).** Both `spawn`
  launch modes — headless (default) and `--windowed` (execs a real terminal
  from `$AOIDE_TERMINAL` instead of detaching) — call it, `--headless`
  aside; do NOT hand-roll a second argv builder for the windowed path, or
  registration/the control socket/the parent-autogate lane can drift
  between the two modes. It is `pub` for ONE caller outside this crate —
  `aoide-server`'s A2A `do_spawn` (P-RSA S10) — and that caller passes
  `headless = true` with `--task` when its own request named one; widening
  it was the point (the door's hand-rolled argv was a second spelling, and
  an A2A child stopped being the wrapper `spawn` produces), so a third
  caller adding a fourth spelling is exactly what this rule forbids.
  `build_terminal_argv` (the `$AOIDE_TERMINAL`
  template parser) is a PURE function on purpose — no env read, no spawn —
  so it stays directly unit-testable; do the env reads (`terminal_template`/
  `require_display`) in the thin callers around it, never inside it.
- **`SessionRecord.origin` is a PERMANENT birth fact stamped ONCE per record,
  the same discipline `headless` already holds (P-P3,
  `docs/architecture/PAIRING.md` decision 7) — never re-derived later. Write
  AUTHORITY is split by SHAPE, not by caller count, tightened at LANE IDENTITY
  P-ID0 (G16/G5).** `session_store.rs::stamp_origin` is `pub` (crosses the crate
  boundary) and is the ONE writer function, with THREE call sites today — but
  the invariant that matters is narrower than "exactly two callers": **a
  `node:*` shape may be stamped from exactly ONE place, `aoide-server`'s
  `a2a::do_spawn` (`stamp_spawn_provenance`)**, called DIRECTLY on the
  just-spawned record — polling for the record's registration on its own budget
  — from the door where the node name IS authenticated. Every OTHER call site
  may stamp a LOCAL-CLASS value but MUST refuse a `node:*` shape, because
  neither has a door behind it: `graph/conduct.rs::session_conduct` reads its
  own inherited `AOIDE_SESSION_ORIGIN` env (a same-uid process can set that on
  itself before invoking `aoide conduct` directly) and `graph/
  resurrect.rs::origin_to_carry` reads a revived session's OWN ledger entry back
  (`state/session-ledger.jsonl` is a plain, same-uid-writable, append-only file
  — a same-uid process can append a line claiming `origin:"node:X"` and then run
  the ungated local `aoide resurrect`, which likewise has no door behind it).
  Both REFUSE (eprintln, never panic) a `node:*` value from their own untrusted
  source instead of stamping it. **A future third-plus call site is fine as long
  as it holds this same refusal** — the invariant is "node:* only from an
  authenticated door", never "count the callers". No `restage_graph()` — like
  `headless`, `origin` is consumed internally (`doc.rs::ledger_session_exit`'s
  projection into the durable ledger, and `origin_to_carry` reading that field
  back to carry a LOCAL-class session's provenance forward onto its revived
  record — G6, same phase, `node:*` excluded per above), not rendered into
  `graph.json`. **This closes the STAMP paths, not the files**: a hand-crafted
  `sessions.json`/ledger line claiming `node:X` is still a readable, unflagged
  string on disk — nothing here makes the files tamper-evident; that is P-ID1
  (the daemon-signed credential, below) — minted and stored, verified on the
  per-session control socket's own accept and consumed by the send gate as of
  P-ID2, and both remaining sockets get a peercred floor of their own as of
  P-ID3 (below). **The raw `origin` field is attribution, not an authenticated
  credential** — a same-uid process can still forge a LOCAL-class origin
  (`stamp_origin` trusts whatever non-`node:*` value it is given). Don't let a
  consumer gate a security decision on the raw field: the authenticated form is
  the sealed credential (below), and the secrets broker's origin gate (P-ID4) is
  the model consumer — it gates on `originClass` read off a VERIFIED seal, never
  the raw `origin` field. The consumer NAME presenting a request stays
  unauthenticated either way (a separate, unbuilt axis — CONTRACTS.md's
  identity-lane accounting). What P-ID0 closes is narrower and real: every
  record-STAMP path this codebase drives now refuses a `node:*` shape it didn't
  mint itself at the door — env AND the unsealed ledger both.
- **`SessionRecord.remoteParent` is stamped from `aoide-server`'s A2A door
  ONLY (remote sub-agents P-RSA S3, CONTRACTS.md §4) — there is no local
  path, and adding one is the mistake this bullet exists to prevent.**
  `session_store.rs::stamp_remote_parent` is `pub` (crosses the crate
  boundary) with exactly one caller, `a2a::do_spawn` via
  `stamp_spawn_provenance`, and the value it stamps is built from the node
  record the request's signature resolved to (`name`, `pubkey`) plus the
  claim the caller signed — never a name or key taken from a header or the
  request body (CONTRACTS.md §6's rung table). Do NOT add a `conduct`/
  `spawn` flag, an env read, or a `resurrect` carry for it: an ambient value
  is exactly the unauthenticated shape the `node:*` refusal above closes one
  field over, and the absence IS the invariant — the test
  (`stamp_remote_parent_is_change_only_and_leaves_parent_session_id_none`)
  pins it, so don't "fix" the missing path by adding one. Change-only, like
  `stamp_origin`/`stamp_seal` (a same-value re-stamp writes nothing; a
  genuinely different value overwrites, since the guard is the value already
  on the record, not a "has this ever been set" flag) — and it never touches
  `parentSessionId`, which stays LOCAL-only: every reader of that field (the
  autogate grant and sibling rule in `graph/send.rs`, `graph/model.rs`'s
  grouping, `graph link`'s cycle check, `taskreport`'s mailbox) treats it as
  a local id, so a foreign value there would dangle at best and grant local
  autogate to a stranger at worst. Attribution, never a gate: the file is
  same-uid-writable, so the key comparison the DOOR makes is what a consumer
  may trust, not this field as read off disk.
- **`SessionRecord.seal`/`sealedIssuedAt` are STAMPED from `aoide-server`
  only, but VERIFIED from inside this crate (LANE IDENTITY P-ID1/P-ID2).**
  `session_store.rs::stamp_seal` is `pub` (crosses the crate boundary) the
  same way `stamp_origin` does, but unlike `origin` it has no local-class
  call site in `conduct.rs` at all — the seal's signing key
  (`aoide_storage::identity::mint_ephemeral`) lives only in the DAEMON
  process's memory (OQ1-A) and this crate has no access to it, so both
  legitimate callers (the `dispatch` handler's `session start` path, and
  the daemon's own tick-driven sweep for directly-registered sessions) live
  in `aoide-server`. Don't add a `stamp_seal` call site in this crate "for
  symmetry with `stamp_origin`" — there is no key to sign with here.
  VERIFYING, by contrast, needs only the PUBLIC key (never secret,
  `aoide_client::daemon::daemon_seal_pubkey_hex` fetches it fresh over
  `ping`) — that half's SEAMS live here, but as of LANE IDENTITY P-ID4
  their BODIES live in `aoide_storage::attest` (the secrets broker's
  origin gate needs the identical walk/verify and `aoide-secrets` cannot
  depend on this crate — that module's doc has the full DAG argument):
  `graph/identity.rs::verify_seal_over`, `attested_sender`, and
  `window.rs::pid_ancestry`/`pid_starttime` are thin delegates keeping
  every `crate::graph` call site and test unchanged. Edit the body in
  `aoide-storage::attest`, never regrow one in a delegate here.
  `graph/send.rs`'s gate and `graph/conduct.rs`'s accept loop are the two
  consumers in this crate — see each file's own module doc.
- **`identity::peer_cred`/`PeerCred` are `pub(crate)`, not `pub(in
  crate::graph)` (LANE IDENTITY P-ID3) — `shellbridge.rs` reuses them
  directly.** Widened once, for exactly the reason `graph.rs`'s own `mod
  identity` doc comment gives: a sibling module reusing the SAME kernel-truth
  primitive beats a second `SO_PEERCRED` read in this crate. **That primitive is
  a Linux/Android capability, never a portable one: every other host takes the
  SAME UNIDENTIFIED `None` an unreadable creds read gives — a named `cfg`
  refusal, never a substitute (`getpeereid` reports no pid, so it could not fill
  the `PeerCred.pid` these gates walk).** Both callers keep their posture:
  `shellbridge`'s accept-time `cross_uid_gate` REFUSES an unidentified peer, and
  the control socket's self-injection check reads it as not-self (that narrow
  guard refuses only true self-injection). Do not widen further to plain `pub`
  "for convenience" — this stays an internal primitive, never crossing the
  `aoide-conduct` -> `aoide` crate boundary root's shim re-exports onward;
  `aoide-server` reuses `aoide_secrets:: peercred` instead (already `pub`,
  already a dependency) rather than reaching into this crate for it, the same
  "small local reimplementation over a one-fn cross-crate edge" discipline
  `identity.rs`'s own module doc states for its relationship to
  `aoide-secrets::peercred`. **The cross-uid floor on shellbridge/`aoided`
  (`cross_uid_gate` in each file, pure, unit-tested without a real different-uid
  connection) does NOT stop a same-uid attacker** — every legitimate connector
  on both sockets already shares the operator's own uid under OQ1-A. Do not add
  a same-uid restriction to shellbridge's verdict door in this crate to "finish
  the job": the legitimate verdict caller is the desktop QML process, which is
  NOT part of any agent session's ancestry, so an ancestry-shaped floor would
  refuse the real caller, not just an attacker — this residual is documented,
  not silently left, in `CONTRACTS.md`'s identity section. `send.rs`'s own gate
  (`real_attested_sender`) resolves identity against whichever process is
  actually running `deliver_local`: a request dispatched through `aoided`'s
  socket resolves against the daemon's own ancestry, not the original caller's.
  Don't "fix" this by threading a peercred pid into `send.rs` without opening
  that as its own scoped phase. A network door never reaches that resolution:
  the walk is skipped for `Door::A2a`, which runs inside aoided with no sealed
  ancestor, so its send parks. An MCP send resolves in the MCP server's own
  process, whose ancestry names who launched the server and not who writes to
  its stdin. P-ID3 closed only the
  ATTRIBUTION half of that gap (`aoided`'s `invocation_from_dispatch_request`
  and `a2a::do_inject` both stamp an absent `from` explicit-empty rather than
  let it fall through to their own process's ambient `AOIDE_SESSION_ID`), not
  the gate's own identity resolution.
- **Conductability is `is_conductable_now` (`doc.rs`: the stored flag AND
  the socket file's own existence on disk) at every reporting AND acting
  boundary, never the stored flag echoed verbatim.** `build_graph`'s
  `conductable` node field derives it at read time (`graph`'s JSON/tree,
  `resolve_graph_document`'s federation wire response); `send.rs`'s own
  gate (`deliver_local_with`) calls the SAME function before ever dialing
  a socket, so a caller never acts on a staler answer than a reporter
  would give. The STORED `SessionRecord.conductable` is a permanent fact
  about a session's NATURE — it IS a conducted PTY wrap — and
  `window.rs`/`reap.rs` classification (`is_agent_kind`, the lineage
  checks) keeps reading that field directly; a session does not stop
  being a conducted wrap just because its socket briefly vanished, so
  don't migrate or clear the stored field to "fix" a stale report.
  `shellbridge.service` owns `$XDG_RUNTIME_DIR/aoide` with
  `RuntimeDirectoryPreserve=yes` (`modules/dendrites/strillix/shellbridge.nix`), so
  an ordinary service restart no longer deletes a live session's socket
  file out from under it — but the on-disk check stays regardless,
  because a socket file can still outlive or predate its process (a
  crashed wrap, a record restored from stage, a runtime directory
  cleared at logout): liveness is judged at every boundary, never from
  the stored flag or a remembered path alone. A missing or empty socket
  path is not-conductable.
- **`mail_bridge` carries NO logic of its own — it stays passthrough
  functions only, forever (P-M2 ruling 1, extended by P-M3's third).** It exists only because
  `aoide-server` may not carry a production `aoide-client` dependency
  while this crate already does; growing a real decision, retry policy, or
  new drain shape inside `mail_bridge` itself — rather than in
  `aoide_client::mail_wire`, which owns the actual drain — would leave two
  crates each partially responsible for one behavior. A change to WHEN or
  HOW a node's outbox drains belongs in `mail_wire`; a change to WHICH
  crates may reach it does not belong here either — that is the manifest's
  job. P-M3's `settle_deposit` is the pattern a further addition must
  follow: a one-line `pub fn` delegating straight to
  `aoide_client::mail_wire`, never a second implementation, and only after
  checking it truly cannot be called directly from `mail_wire` by whoever
  needs it.
- **The session sweep and the roster-exit letter drop are not `mail_bridge`
  functions, and do not become them.** They read the roster, so they live in
  `graph/session_store.rs` (`sweep_session_letters`, `drop_ended_sessions`),
  re-exported through `graph.rs`. Two orders hold there. The exit paths call
  `drop_ended_sessions` after their roster write returns, never under the
  roster lock. The sweep reads the sessions its letters name BEFORE the roster;
  reversed, a live session's letter can look gone. A roster that cannot be read
  skips the session pass; it is never taken as empty.
- **Aoide writes only the unit pair it names and the record.** `add` refuses
  a unit slot that is taken, a link included (`slot_taken`), and a unit
  directory that is a link or a file (`prepare_unit_dir`), so nothing is
  written through either. `remove` deletes only a unit that still hashes to
  the record (`unit_state`). It refuses, deleting nothing, when a unit was
  edited or is a link, and it skips a unit that is already gone. Two windows
  stay open to a same-uid writer. `add` checks each slot, then renames its units
  into place: a file that appears in a slot after the check is replaced, and a
  link there is written through, since storage's `atomic_write` follows one link
  at its target. `remove` runs a bounded
  `disable --now` between its hash check and its delete, so a unit swapped into
  that gap is deleted, and `remove` still reports it removed. Pinned by
  `a_symlinked_or_foreign_unit_in_the_way_is_refused_and_never_written_through`,
  `a_job_whose_units_are_both_gone_is_removed_and_its_name_can_be_added_again`
  and
  `a_symlinked_unit_counts_as_changed_and_remove_refuses_it_and_deletes_nothing`
- **The record holds job names only, each once.** Every read of the record goes
  through `load_jobs`, which refuses a record holding a name that is not a job
  name, or one name twice; a named `runs` reads no record and checks `--name`
  with `valid_name` instead. Pinned by
  `a_record_holding_a_name_that_is_not_a_job_name_refuses_each_verb`,
  `a_record_that_names_one_job_twice_refuses_each_verb` and
  `a_record_holding_a_bad_name_refuses_add_before_any_write`.
- **A unit's text comes from `unit_texts` alone, one `quote_word` per argv
  word.** `WorkingDirectory=` is unquoted with its `%` doubled, and it appears
  only with `--cwd`. `ExecStart=` doubles `$` as well as `%`. `Environment=`
  takes no variable expansion (systemd.exec(5)), so it doubles `%` and not `$`.
  Pinned by `unit_texts_quote_each_word_the_systemd_way` and
  `a_job_without_a_cwd_has_no_working_directory_line`.
- **The four verbs open to the `Cli` and `Daemon` doors only.** `add` and
  `remove` also refuse a door that cannot approve. Every verb refuses before
  its first write when the host is not Linux or `systemctl --user show` fails.
  `add` and `remove` make that probe before they take the stage lock, and each
  tool call is bounded by `JOB_TOOL_WAIT` (10 s, plus up to 2 s while its pipes
  drain; a child in uninterruptible sleep is never reaped, and its call waits
  with it). A call that gives no answer is never read as a no. Pinned by
  `the_job_verbs_open_to_the_cli_and_daemon_doors_only`,
  `a_host_that_is_not_linux_refuses_every_verb_and_writes_nothing`,
  `a_failing_user_manager_probe_refuses_add_with_zero_files` and
  `a_probe_that_never_exits_refuses_add_in_bound_and_leaves_the_lock_free`.
- **A journal line is data, and a job's rows come from its own unit.** `runs`
  asks the journal once per recorded job, in name order, until a call gives no
  answer; each call is for that job's own unit (`_SYSTEMD_USER_UNIT=`), so no
  job's window holds another's rows. It keeps a line only when its `MESSAGE` is
  one JSON object with a known status, and its own `_SYSTEMD_USER_UNIT` names
  the job: a `UNIT` or `USER_UNIT` field a client sets names no job. Without a
  name, a call that exits non-zero lists only its own job under `unread`, and
  the first call that gives no answer also ends the reads, so a silent journal
  costs one bound whatever the job count: each later job is `unread` as not
  asked. When no call answers, the verb refuses with the first failure; a named
  run refuses on any failure. Its strings reach `--json` as JSON strings, and
  the human rows pass through `visible`. Pinned by
  `runs_reads_each_job_on_its_own_and_orders_rows_oldest_first`,
  `runs_lists_a_job_whose_journal_call_exits_non_zero_under_unread`,
  `runs_makes_one_silent_call_and_refuses_as_failed_when_nothing_answers`,
  `runs_ends_its_reads_at_the_first_silent_call_and_lists_the_rest_as_not_asked`,
  `runs_refuses_as_failed_when_every_recorded_job_exits_non_zero`,
  `runs_with_no_recorded_job_makes_no_journal_call`,
  `runs_turns_a_journal_fixture_into_rows_and_shows_their_text_visibly` and
  `a_run_row_needs_its_own_unit_so_a_client_set_unit_field_gives_none`.
- **No test reaches the real user manager, journal or unit directory.** The
  shimmed tests point `XDG_CONFIG_HOME` at their own root and put the
  `systemctl`, `journalctl` and `systemd-analyze` shims first on `PATH`,
  under the crate's env lock.

## Extension points

- **`resurrect` writes exactly ONE audit line per invocation
  (`audit_resurrect`, U2) — never zero, never one per candidate/spec.**
  Every return past a pure usage/stage-file miss (an unresolved project
  name, an unresolved `--id`, an empty selection, the final per-candidate
  or per-spec loop outcome, in either mode) calls it once; a pure
  `require_flag`/`stage_error` early exit does not, the same posture
  `send.rs`'s `audit_send`/`pending.rs`'s `audit_pending` already hold
  toward their own early exits. The gate-6 finding this closes: an empty
  bare-mode selection used to return before ever reaching an audit call at
  all — don't reintroduce a return path between "a real decision was made"
  and the `audit_resurrect` call that reports it.
- **A new `graph`/`conduct`/`hooks` command** adds a `cmd!`/`register` entry in
  `commands/`, wired into `cli`'s `commands::all()` (this crate's commands are
  core, never `strillix`'s).
- **A new per-session socket convention** (the channel socket,
  `channel_socket_path`, P-M5c-2, is the first one beside
  `conduct_socket_path`) stays a pure path function in `graph/conduct.rs`
  sharing `conduct_socket_path`'s runtime-dir resolution — never a second
  `XDG_RUNTIME_DIR`/fallback lookup hand-rolled elsewhere. Whether this
  crate itself binds the socket or only computes the path for another
  crate to bind (the channel socket's case: `aoide-server` owns bind/
  accept/unlink) is a per-socket decision, stated in that function's own
  doc comment.
- **A new `session grant` kind** (`exempt`, task #20, is the second one
  landed after `undying`; #127's secret grants are the next one named, not
  yet built) adds one `match` arm in `grant.rs::session_grant` — no new
  registered path, no `commands/` entry: `<kind>` is a positional argument
  on the ONE `["session", "grant"]` path, `secrets automate <name> on|off`
  style. A picker is OPT-IN, not mandatory — `exempt` has none at all (bare
  `session grant exempt` is a taught refusal naming the scripted form
  directly, since its caller is a script, not a tty session). If a new kind
  DOES want a picker, reuse `grant.rs::require_cli_tty`'s CLI+tty gate shape
  rather than re-deriving it — there is still only one multi-select
  primitive (`aoide_protocol::pick::choose_many`) in this crate.
- **A new hook event or harness profile** extends `aoide_protocol::agents`,
  not this crate — the harness-profile table lives one layer down.
- **A new `job` verb** is a `fn(inv, host: Host)` beside the others in
  `graph/job.rs`, reached through a `pub fn job_*(inv)` that passes `HOST`.
  The `Host` argument is what lets a test run it as `Host::Other`. Its probe
  runs before its first write. Its `cmd!` goes in `commands/job.rs` and is
  wired into `cli`'s `commands::all()`, as any `commands/` entry is.

## Docs update required in the same commit

- This `README.md` when a public module, seam, or charter-smudge reasoning
  changes.
- `CONTRACTS.md` when a graph/session wire shape changes.
- `graph/job.rs` changes to a unit text, the `jobs.json` shape or a verb's
  output: `CONTRACTS.md`'s `state/stage/jobs.json` section, and the local-jobs
  seam bullet in this crate's `README.md`.
- `pkgs/aoide/crates/AGENTS.md` for cross-crate invariants — not restated
  here.
- `doorbell.rs` changes update `docs/architecture/MAIL.md`'s "Delivery and
  the doorbell" section — that document is the design's canonical prose
  statement, this file only the invariants an editor must hold.
- `pingback.rs`, `pingback/hook.rs` and `pingback/prompt.rs` changes update `docs/architecture/EIDOLON-TRACE.md`'s
  "Second slice" section (the ping-back's canonical prose statement), the
  `state/stage/pingback.json` shape in `CONTRACTS.md` §4, and
  `docs/Aoide-Wiki/concepts/orchestration/Conductor-Channel.md`'s
  "Child-of-target (reciprocal)" paragraph. A change to the event/line split
  or to what a remote child spools also updates `CONTRACTS.md`'s
  `state/stage/pingback-remote.json` (§4) and its ping-back-history paragraph
  (§6) — the reaper that writes the ring and the door that serves it are two
  ends of the one shape. A change to the PULL — the ledger fields it reads or
  writes (`linesAfter`, `drained`), what it re-validates, or the tag it
  renders — updates `CONTRACTS.md`'s `state/stage/remote-children.json` (§4),
  `docs/Aoide-Wiki/concepts/orchestration/Managed-Task-Wrapper.md`'s "How the
  report is delivered" (the remote-parent paragraph), and `EIDOLON-TRACE.md`'s
  "Second slice" beside the spool half: the two lanes are one conversation,
  and a doc that describes only the sending end is half a page.
- `prompt.rs` or the screen half of `conduct.rs`'s tick changes update
  `docs/Aoide-Wiki/concepts/orchestration/Session-Graph.md`'s "Programs with no
  harness — the screen" (the table and the quiet are written there too) and
  the `question` and `needsSudo` fields in `CONTRACTS.md` §4.
- A change to shellbridge's replying verbs — the `sessionaction`/`projectaction`
  whitelists and reply shapes, or the `sessiontrace` read — or to the fire-and-forget
  core verbs updates `ShellBridge.qml`'s protocol comment
  (`pkgs/strillix-shell/qml/ShellBridge.qml`, which names the verbs it answers) and
  `concepts/cli/Doors-and-Nodes.md`'s socket-command list, in the same commit. A
  desktop verb's wire is strillix's, and its AGENTS.md names the same two places.
- **`graph.json`'s compositor block (`doc::workspace_block`) is ONE seam.**
  `build_graph` publishes it and `aoide workspace list --json` prints it from
  the same function — never a second builder, never a reader re-deriving the
  rows, because the two doors disagreeing about a binding is exactly the bug
  this seam exists to prevent. The block rides only when some session reports a
  workspace or some project has a binding, and a session node's `activeAt`
  rides under the SAME gate (it feeds the block, so it travels with it): a
  document with no compositor in play must stay byte-identical, test or no
  test (`nothing_in_play_leaves_the_document_byte_identical`). Rows and ties
  are built from LOCAL sessions only — the input IS the local roster, and
  another node's rows arrive nested under a fresh node's `children`, which the
  block never reads. Both lineage walks (a headless session's activity folding
  into its nearest windowed ancestor's row, and a spawned tie's leaving end)
  go through `windowed_ancestor`, which is visited-set bounded: a malformed
  parent cycle must never spin the builder. A headless session's pulse folds
  in as ACTIVITY ONLY — no session id joins that row's `sessions`, no count
  moves, and no tie is drawn. `projects.json` stays the RECORD; this block is
  its projection, so a binding change belongs in `workspace set`/`clear` and
  nowhere else.
- A change to that block's shape, its gate, or its definitions updates
  `CONTRACTS.md` §4's `state/stage/graph.json` section (the field tables),
  `docs/Aoide-Wiki/concepts/orchestration/Session-Graph.md`'s bindings
  paragraph, and `conduct/README.md`'s workspace-binding bullet, in the same
  commit — the paint layer reads this shape and has no schema of its own.

## Managed task wrapper (`spawn --task`, `session watch`)

- **The instruction sidecar is written once, 0600, and carries only the
  operator's bytes.** `state/sessions/<id>.instructions.md` is created
  exclusively by `spawn` BEFORE the child re-execs; a second write for the same
  session id is a taught error naming the stale file, never a raw io error and
  never a silent overwrite. No credential, token or environment dump is ever
  appended to it. The child reaches it by PATH (`AOIDE_TASK_INSTRUCTIONS`,
  `instructionsPath`), never by re-derivation.
- **`session watch` is read-only and cursor-free.** It never calls
  `write_stage`, `mark`, `read_for`, `ring` or writes the conduct socket; its
  only mail reader is `mail::unread_for(slug, Some(child))` — a reader-selected
  PEEK that advances no cursor — so watching consumes neither the child's nor
  the observer's unread state, and the rail is the CHILD's own queue rather
  than every letter in the mailbox. Every untrusted fragment it renders is
  sanitized (control characters stripped, lines clipped) — including a sender's
  id, wherever it is printed — and raw PTY bytes go only to a real terminal.
- **A watch of `<node>/<query>` reads the far frame; it never reads the far
  disk.** The resolution is `graph::remote`'s (one definition, shared with
  `send --to`: the node's CACHED graph, a taught `node pull` refusal when there
  is none), and the read is one signed `tasks/get`
  (`aoide_client::commands::task_get_on_node`, 512 KiB cap, tunnel keyed
  `(session, node name)` like every other node action). Every string that
  arrives is re-cleaned by `Frame::clamp_untrusted` and every count re-clamped
  to the LOCAL bounds — the requested tail, `MAIL_RAIL`, `BLOCK_LINES_MAX` —
  before a byte is rendered; **raw PTY bytes have no path at all on the remote
  side**, whatever stdout is, because there is no terminal gate there to open.
  `suggested` is rebuilt locally (`aoide send --to <node>/<id> --submit -- …`),
  never taken from the frame, and the header carries the node. `logPath`,
  `socket` and `instructionsPath` are STRUCK (`None`) — they name paths and a
  control socket on the box that wrote the frame, so a peer's invented
  `/etc/shadow` never renders as `log …` (the instruction TEXT still shows;
  the renderer has an arm for a block whose path is gone). Live polls every
  2 s and ends on the far record's own `presence != running`, or Ctrl-C: no
  cursor is kept on either box, so a remote watch is exactly as read-only as a
  local one. **A FAILED poll ends the watch with the structured refusal** —
  `Status::Error` carrying its own `reason`/`code` plus `followed: true` — never
  an `ok` that hides the diagnosis, and no prose is printed on a `--json`
  stream (that applies to the Ctrl-C report too). A peer's own error text
  (`FrameReadError.message`, and `send`'s remote-delivery failure) goes through
  `common::clean_line` before it is printed — an OSC-52 escape or a bidi
  override in a far node's message is a terminal instruction, not a message —
  as do the petname/session id `remote::node_session_label` renders out of
  another box's cached document. The far door's `-32011` refusal is rendered as
  a taught error naming the grant and the box it runs on — never as a bare
  refusal, and never as "the session does not exist".
- **A finished run's exit report is filed by the daemon tick through the
  registered `mail.send` implementation** (`Door::Daemon`, in-process — the
  daemon authority path, not a bypass), never by the mail layer called directly
  from conduct and never by the dying child. The tick's trigger is the record
  (`task` set, `state = done`, no cursor entry), so a daemon that was down
  reports on its next pass.
- **Delivery semantics: single delivery in the normal path, never lost, and one
  stated duplicate window.** The cursor (`state/stage/taskreport.json`) advances
  only after the letter is really in the mailbase, so a filing failure is
  retried on the next tick. The guarantee is **at-least-once, normally exactly
  once**: a crash between the filing and the cursor write re-files, and the
  retry mints a new msgid, so the mailbase's own duplicate memory cannot
  collapse it. Those words — at-least-once, normally exactly once — are the
  whole delivery description of this lane; no stronger guarantee is claimed.
- **An unfiled task run is not prunable.** `prune_done` skips a `task`-carrying
  `done` record with no cursor entry, so neither the reaper's sweep nor
  `session prune` can drop the report lane's own trigger before it fires.
- **A signal death is not an exit code.** The record's `exitCode` is written
  from `ExitStatus::code()`, which is `None` for a signal death; a killed run
  keeps `endedAt` and no `exitCode`, and nothing may read that absence as `0`.
  A signal death carries no `exitCode` in the command envelope either: the
  field is inserted only when there is a real status, so no consumer ever sees
  a fabricated `-1`.
- **One wake per event, and its reason is kept.** The lane takes the mail-side
  doorbell once; no second raw line is injected. The ring's own outcome
  (`rang:N`, `deferred:<state>`, `skipped:<reason>`, or its failure) is recorded
  in the cursor and shown by the view — a never-conducted (Codex desktop/app)
  parent cannot be woken at all, and that is reported rather than papered over.
- **The live-slug check is a courtesy, not a lock — and it is ONE predicate
  with TWO callers.** `live_run_for`/`live_run_refusal` (`graph/spawn.rs`, both
  `pub`, re-exported at `graph::`) are consulted by `spawn --task` and by
  `aoide-server`'s A2A door (P-RSA S10 review, M2): the door composes `conduct`
  directly, so sharing this is what keeps a peer's child from squatting a slug
  the operator owns — never write a second copy of the refusal text in the
  server. `spawn --task` refuses a slug whose record is not `done`, but two
  concurrent spawns can both pass it; nothing afterwards enforces uniqueness,
  and no code claims it. In that race
  both records carry the slug and the mailbox holds both runs' letters, kept
  apart by each letter's `from` session id.
- **The child is its task mailbox's own reader, and channels are distinct.**
  `spawn`/`conduct` enrol the RUN's session id under the slug, so a mailbox
  records what was sent TO the subagent against the subagent's own cursor. The
  run's exit report is filed to the REPORT mailbox (`--report-to`, else the
  parent's id when that is a legal mailbox name, else the documented role
  mailbox `conductor`) — never to `self/<slug>`, whose only contents are the
  letters sent to the child. An id that is not a legal mailbox name is never
  rewritten into one: two identities must not collide on a name.
- **The only reader-selected mail read is a PEEK.** `aoide_storage::mail::
  unread_for(name, reader)` returns `(mark, entries)` and writes nothing — no
  `save_cursors`, no mark created, no `read_for`/`mark`. `session watch` uses it,
  so the observer consumes neither the child's mail nor its own, and the live
  view prints `child read through seq <n>` when the CHILD's own mark advances.
- **`--timeout` is the supervisor's WALL CLOCK, never inactivity.** It is
  checked on every multiplex iteration and bounds the `poll` wait
  (`min(remaining, poll_timeout)`, checked arithmetic both sides). On expiry the
  wrapper kills the DIRECT child it spawned (`Child::kill`, never a
  roster-resolved id or an ancestry walk) and reads the post-kill status as
  `killedWith` — secondary evidence, never the result. A silent child is alive
  until the deadline; a child printing continuously is still killed at it.
- **The end is a CLOSED vocabulary, and a kill is never an exit code.**
  `outcome` is `exit` (with `exitCode`), `signal` (with `signal`; no
  `exitCode`), `timeout` (the wrapper's own act), or `stopped` (no status:
  reaped or killed outside the wrapper). `exitCode` is written only for
  `exit`; the old `code().unwrap_or(-1)` collapse is gone, and
  `do_session_end`/the reaper stamp `stopped` for a run they never saw exit.
  Nothing in this path reads a harness trace, so completion reporting does not
  depend on the ping-back (VV/JEV) lane.
- **A completed task run is HISTORY, not garbage — but a DOOR SUMMON's history
  is not this box's (P-RSA S10 review, H1).** Routine cleanup retains every
  `task`-carrying `done` record THIS BOX ran — `prune_done` cannot drop one,
  filed or not — so a finished run stays resolvable by `session watch`
  (instructions, the child's unread queue, output, outcome, report). Only the
  explicit `aoide session prune` may drop a FILED task run; an unfiled one is
  retained even there, and after any explicit prune the durable history (ledger
  line, letters, PTY transcript, instruction sidecar) is still on disk. A record
  whose `origin` is a `node:*` value was created by the A2A door on a PEER's
  request, and retaining those forever made the roster a remote caller's to
  grow (one `metadata["aoide/task"]` spawn per request, each ending `done`),
  permanent and revocation-proof — unpairing the peer does not sweep them. So a
  door summon is retained only while its report is still owed (the cursor entry
  is the lane's own trigger, as for an unfiled local run) and is swept once
  filed, by the automatic sweep AND by `session prune`; its durable history
  stays exactly as a pruned local run's does.
