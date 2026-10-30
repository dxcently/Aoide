# pkgs/aoide-desktop/default.nix — the desktop workspace's gate.
#
# `nix flake check` builds this package (`pkg-aoide-desktop`) from the committed
# tree. `preCheck` lints the product configuration and the gate's form with
# clippy, then `checkPhase` runs every test of the workspace, with every
# feature, over that fresh build, except the names in `checkFlags`. A dk.sh run
# is a pre-check: it may read a stale dep-info file, and it proves nothing about
# the commit.
{
  lib,
  stdenv,
  rustPlatform,
  clippy,
}:
let
  subdir = "aoide-desktop";
  target = stdenv.hostPlatform.rust.rustcTargetSpec;
in
rustPlatform.buildRustPackage {
  pname = "aoide-desktop";
  version = (builtins.fromTOML (builtins.readFile ./crates/shell/Cargo.toml)).package.version;

  # This directory, and the two paths its build reads outside it: pkgs/aoide's
  # root manifest (protocol inherits its workspace fields), and protocol, which
  # is exec's path dependency.
  src = lib.fileset.toSource {
    root = ./..;
    fileset = lib.fileset.unions [
      ./.
      ../aoide/Cargo.toml
      ../aoide/crates/protocol
    ];
  };
  cargoRoot = subdir;
  buildAndTestSubdir = subdir;
  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ clippy ];

  # The build phase's target, so the lints read the same artifacts the tests do.
  preCheck = ''
    pushd ${subdir}
    cargo clippy --offline --frozen --release --target ${target} --workspace -- -D warnings
    cargo clippy --offline --frozen --release --target ${target} --workspace --all-targets --all-features -- -D warnings
    popd
  '';

  cargoTestFlags = [
    "--workspace"
    "--all-features"
    "--no-fail-fast"
  ];

  # The tests that fail in this sandbox, skipped by exact name (`--exact`). Its
  # `/` belongs to uid 65534, which the exec crate's pin rule refuses. The
  # gate's `dk.sh` runs these tests. A test not listed here runs in the sandbox.
  checkFlags = [
    "--exact"
    "--skip"
    "tests::a_child_that_ignores_sigterm_is_refused_after_the_exit_bound_and_is_alive_then"
    "--skip"
    "tests::a_grandchild_holding_stdout_is_refused_at_the_deadline_and_no_signal_reaches_it"
    "--skip"
    "tests::a_program_in_a_private_dir_under_a_sticky_other_writable_dir_is_pinned"
    "--skip"
    "tests::a_setuid_or_setgid_program_is_refused"
    "--skip"
    "an_exec_failure_is_reported_as_an_error"
    "--skip"
    "the_door_env_points_the_door_at_its_runtime_and_drops_the_session"
    "--skip"
    "a_401_is_an_error_and_keeps_the_card"
    "--skip"
    "a_204_keeps_the_card_until_the_door_settles_the_ask"
    "--skip"
    "a_409_is_not_pending_and_keeps_the_card_until_the_settle"
    "--skip"
    "a_busy_close_leaves_the_watch_and_its_turn_state"
    "--skip"
    "a_422_is_rejected_with_the_doors_reason_and_keeps_the_card"
    "--skip"
    "a_banner_past_64_kib_is_refused_at_once_and_its_door_is_signalled"
    "--skip"
    "a_changed_ask_is_refused_and_a_yes_is_sent_only_with_the_digest_on_screen"
    "--skip"
    "a_cancel_whose_turn_never_ends_is_no_turn_end_and_signals_nothing"
    "--skip"
    "a_close_after_turn_that_leaves_the_door_live_withdraws_the_mark"
    "--skip"
    "a_close_that_exits_sends_no_gone_or_dropped_within_a_second"
    "--skip"
    "a_cwd_that_is_relative_missing_not_canonical_or_holds_u202e_is_refused"
    "--skip"
    "a_close_due_for_a_door_idle_while_the_channel_is_full_still_arrives"
    "--skip"
    "a_door_that_ignores_its_first_sigterm_stays_watched_as_still_live"
    "--skip"
    "a_door_that_outlives_its_close_is_not_asked_to_close_again_when_a_later_turn_ends"
    "--skip"
    "a_close_whose_cwd_moved_is_unverified_and_keeps_the_record_and_the_watch"
    "--skip"
    "a_flood_that_nothing_drains_holds_the_stream_and_the_channel_holds_no_more_than_its_bound"
    "--skip"
    "a_killed_apps_door_reattaches_from_the_record"
    "--skip"
    "a_forged_record_whose_cmdline_is_not_the_launch_argv_is_dropped_and_never_signalled"
    "--skip"
    "a_hello_that_fails_once_keeps_the_record_and_the_next_open_reattaches"
    "--skip"
    "a_forged_record_with_a_wrong_start_time_is_dropped_and_never_signalled"
    "--skip"
    "a_doors_file_that_does_not_parse_is_refused_and_never_rewritten"
    "--skip"
    "a_reconnect_answered_with_a_500_then_properly_delivers_one_unwatched_and_resumes"
    "--skip"
    "a_launch_whose_hello_fails_keeps_its_record_unwatched_and_signals_nothing"
    "--skip"
    "a_record_whose_hello_names_another_session_is_kept_unwatched_and_never_signalled"
    "--skip"
    "a_reconnect_whose_hello_names_another_session_is_unwatched_and_keeps_its_record"
    "--skip"
    "a_door_launched_through_a_link_is_closed_after_the_link_is_repointed"
    "--skip"
    "a_record_whose_pid_is_dead_is_dropped"
    "--skip"
    "a_record_without_a_program_does_not_parse_and_the_open_is_refused"
    "--skip"
    "a_record_whose_program_is_not_the_program_its_process_runs_is_dropped_and_never_signalled"
    "--skip"
    "a_stop_now_on_a_channel_drained_after_a_pause_closes_the_door"
    "--skip"
    "a_same_id_ask_sent_again_with_another_tool_is_refused_at_the_first_confirm"
    "--skip"
    "a_stop_now_on_a_channel_nobody_reads_is_no_turn_end_and_the_door_lives"
    "--skip"
    "a_stop_now_whose_close_leaves_the_door_live_withdraws_the_mark"
    "--skip"
    "a_stopped_watch_sends_nothing_once_its_doors_are_dropped"
    "--skip"
    "a_stream_that_ends_in_a_turn_revokes_the_mark_and_no_close_is_due_after_its_reconnect"
    "--skip"
    "a_revoked_mark_is_never_signalled"
    "--skip"
    "a_turn_extended_by_a_new_one_is_not_signalled_until_the_last_ends"
    "--skip"
    "a_stream_that_ends_while_its_door_lives_is_opened_again"
    "--skip"
    "a_stream_that_ends_with_its_door_gone_drops_the_record"
    "--skip"
    "a_changed_pin_leaves_a_running_door_its_record_and_its_watch"
    "--skip"
    "a_yes_above_the_cap_is_taken_only_in_the_apps_pane_under_its_digest"
    "--skip"
    "an_ask_settled_while_the_stream_is_down_is_not_replayed_on_reopen"
    "--skip"
    "a_store_directory_that_links_to_nothing_refuses_the_open"
    "--skip"
    "a_symlinked_doors_json_is_refused_and_never_rewritten"
    "--skip"
    "an_idle_close_leaves_no_token_file_and_removes_the_record"
    "--skip"
    "every_banner_refusal_signals_its_own_door_and_names_the_refusal"
    "--skip"
    "an_unset_or_refused_runtime_dir_opens_nothing_and_starts_no_door"
    "--skip"
    "an_answer_goes_only_to_the_door_whose_stream_carried_its_ask"
    "--skip"
    "no_banner_within_30_seconds_is_refused_and_its_door_is_signalled"
    "--skip"
    "a_store_directory_that_is_a_link_is_refused_and_nothing_is_written_through_it"
    "--skip"
    "close_after_turn_while_the_turn_state_is_unknown_is_refused_and_signals_nothing"
    "--skip"
    "a_reconnect_after_a_changed_pin_keeps_the_door_and_its_record"
    "--skip"
    "a_wrong_token_path_keeps_its_record_unwatched_and_signals_nothing"
    "--skip"
    "stop_now_after_a_reconnect_closes_on_the_409"
    "--skip"
    "an_answer_to_a_door_that_fails_its_checks_sends_no_request"
    "--skip"
    "stop_now_cancels_before_it_signals"
    "--skip"
    "the_door_is_not_this_process_child_and_leads_a_session_of_its_own"
    "--skip"
    "stop_now_on_a_door_that_fails_its_checks_sends_no_request"
    "--skip"
    "stop_now_on_an_idle_door_closes_at_once_on_a_409"
    "--skip"
    "unknown_vault_names_refuse_a_launch_before_anything_starts"
    "--skip"
    "open_returns_a_later_records_dropped_while_the_channel_is_full"
    "--skip"
    "the_record_holds_the_launch_fields_and_is_mode_0600"
    "--skip"
    "sixty_five_asks_forward_sixty_four_and_send_one_overflow_for_the_65th"
    "--skip"
    "a_zombie_record_is_dropped_with_an_event_naming_its_pid"
    "--skip"
    "a_bad_token_env_is_shown_through_visible"
    "--skip"
    "a_call_past_its_deadline_is_refused_as_a_timeout"
    "--skip"
    "a_deadline_longer_than_the_default_runs_under_the_default"
    "--skip"
    "a_declared_flag_reaches_the_child_in_its_own_form"
    "--skip"
    "a_later_longer_deadline_does_not_lengthen_an_earlier_one"
    "--skip"
    "a_non_zero_exit_returns_its_stderr_verbatim"
    "--skip"
    "a_program_that_will_not_print_its_schema_is_refused_at_load"
    "--skip"
    "a_token_env_of_any_length_or_in_lower_case_is_accepted_and_scrubbed"
    "--skip"
    "a_token_env_the_app_may_not_read_refuses_every_verb_with_no_child"
    "--skip"
    "an_undeclared_flag_starts_no_child"
    "--skip"
    "an_unreadable_config_refuses_every_verb_but_config_with_no_child"
    "--skip"
    "output_at_the_cap_is_kept"
    "--skip"
    "each_child_gets_only_its_scrubbed_environment"
    "--skip"
    "stdin_is_closed_unless_bytes_are_given"
    "--skip"
    "stderr_past_the_cap_is_refused"
    "--skip"
    "stdout_past_the_cap_is_refused"
    "--skip"
    "spawned_text_that_starts_with_dashes_or_holds_equals_or_newlines_stays_positional"
    "--skip"
    "a_good_file_pins_to_its_canonical_path"
    "--skip"
    "a_hook_only_session_is_refused_and_each_case_says_why"
    "--skip"
    "a_line_arrives_spelled_and_a_line_that_is_not_json_arrives_as_other"
    "--skip"
    "a_session_with_a_pty_and_no_trace_is_watched_and_its_frame_comes_first"
    "--skip"
    "a_session_whose_trace_answers_is_followed_with_follow_and_tail"
    "--skip"
    "say_scrubs_the_session_id_and_both_autogate_switches_from_the_send_child"
    "--skip"
    "every_case_runs_a_verb_the_schema_declares"
    "--skip"
    "say_sends_with_its_id_submit_and_yes_and_the_text_as_one_positional"
    "--skip"
    "text_that_starts_with_dashes_and_holds_a_newline_stays_one_positional"
    "--skip"
    "the_envelope_a_follow_prints_at_its_end_arrives_and_the_reader_survives_it"
    "--skip"
    "a_held_child_gets_only_its_scrubbed_environment"
    "--skip"
    "a_held_child_streams_its_lines_before_it_exits"
    "--skip"
    "an_undeclared_flag_starts_no_held_child"
    "--skip"
    "dropping_a_follow_interrupts_its_child_and_reaps_it"
    "--skip"
    "ending_a_follow_interrupts_the_child_which_prints_its_end_line_and_is_reaped"
    "--skip"
    "a_line_past_the_cap_is_dropped_and_counted_and_the_next_line_arrives"
    "--skip"
    "a_blocking_path_through_call_is_refused_and_starts_no_child"
  ];

  meta.description = "The aoide desktop workspace, gated by its tests and clippy runs";
}
