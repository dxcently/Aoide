# AGENTS.md: aoide-desktop-app

Invariants for this crate. The layout is in README.md; the workspace rules are
in `../../AGENTS.md`.

## Invariants

- The command set is one set in four places: `COMMANDS` in `build.rs`, the
  `generate_handler!` list in `src/lib.rs`, the `#[tauri::command]` functions in
  `src/commands.rs`, and the `allow-*` permissions in `capabilities/main.json`
  (kebab-case; `smoke_note` is granted in `capabilities/smoke.json`).
  `every_handler_command_is_in_the_manifest_and_granted_to_main` compares them.
  Adding a command means changing all four in one commit. A smoke-only command
  carries `#[cfg(feature = "smoke")]` on its function and its handler entry,
  and stays in `COMMANDS` in every build, since its permission is generated
  either way.
- `build.rs` picks the capability pattern: `capabilities/main.json` alone, or
  `capabilities/*.json` under the `smoke` feature. Only a smoke build reads
  `smoke.json`.
- The window navigates only to its own origin: `navigation_allowed` admits
  `tauri://localhost` (the app's URL on Linux has no slash) and refuses every
  other URL (`tests/navigation.rs`).
- Every Tauri command is `async` (`every_tauri_command_is_an_async_fn`). A
  synchronous command would run on the main thread, where the confirm dialog
  must wait.
- The window grants only `palette::main_grants`. A call passes `palette::check`
  before any confirm or child. The floor's reason text reaches the person.
- One confirm at a time: `Window::ask` takes `busy` and refuses a second ask.
- `chat_close` joins the reader before it returns. A reader ends by stop flag,
  then interrupts and reaps its child through `Chat::close`.
- The page draws every string with `textContent` or a text node. A construct
  that parses markup from a string, runs a string, or sets an event handler
  property is a gate failure (`the_page_parses_no_markup_from_a_string_and_runs_no_string`),
  and a new one the scan should bar goes into `tests/page.rs` with a planted
  case (`the_scan_flags_each_construct_it_bars`). The page is titled `Aoide` and
  so is the window; `main_window` is the one place the window is built, and the
  smoke run reads the real window's title (the mock runtime keeps none).
- `chat_say` names the harness the session was listed with and sends nothing to
  a session it cannot list (`a_send_names_the_harness_the_session_was_listed_with_and_a_shell_names_none`,
  `a_send_to_an_unlisted_session_is_refused_and_starts_no_send`). A send that
  exits non-zero is an `Err`, so the page shows an error, never a bubble
  (`a_send_core_refuses_reaches_the_page_as_an_error_and_not_a_reply`).
- Every string the window pushes or replies with passes `visible` (or `spell`
  for JSON). `every_pushed_string_is_spelled` holds that for a chat line. An ask
  string is core's visible form already, so it is not spelled again: a row that
  holds a raw character is not pushed as an ask
  (`every_pushed_ask_string_is_spelled_and_a_raw_one_is_no_card`).
- The window follows the asks with one held child. `asks_close` joins the reader
  before it returns, and a reader ends by stop flag, then interrupts and reaps
  its child through `Watch::close` (`the_asks_watch_pushes_each_ask_and_close_ends_it_by_interrupt`,
  `a_second_asks_open_leaves_one_child`). The window's record of the asks is
  built from core's rows only.
- `ask_answer` takes its fingerprint, its session and its ask from that record
  and nothing from the page but the label and a digest: no argument carries a
  fingerprint, and one a page sends is not read
  (`a_fingerprint_the_page_sends_is_never_used`). The confirm rules are
  `asks.rs`'s: a yes asks the confirm on the whole visible call
  (`a_no_skips_the_confirm_and_a_yes_is_confirmed_once_on_the_whole_visible_call`);
  a cancel starts no child (`a_confirm_that_is_cancelled_starts_no_answer`);
  above the cap only the pane's digest of the dialog built now takes the yes,
  and a digest never replaces the confirm below the cap
  (`a_yes_above_the_cap_is_taken_only_in_the_apps_pane_under_the_digest_of_what_it_showed`,
  `a_digest_from_the_page_never_replaces_the_confirm_below_the_cap`). Core's
  refusal is an `Err` with core's message and the ask stays held
  (`a_refusal_from_core_reaches_the_page_as_an_error_and_the_card_stays`).
- `run()` takes no argument unless the `smoke` feature is on. A build without
  it refuses any argument by name, with exit 2, before GTK starts. Under
  `smoke` the one argument is `--smoke <dir>`.
- Under `smoke` every command, through `off_thread`, appends its name and
  launch number to `<dir>/record.jsonl`, never its text. `smoke_note` appends
  its text as data, cut at 4 KiB. The app writes nothing else into `<dir>`
  except `ready`, and reads `scenario.json` for its confirm and its `rows`
  (`src/smoke.rs`). Its one other voice is the two notes it writes once the page
  has loaded: the window's title and the page's DOM summary.
  A confirm whose scenario file is missing or unreadable answers no.
- `permissions/autogenerated/` is committed and must equal what tauri-build
  generates, byte for byte. The workspace gate's `perms` step copies the
  committed tree, builds the app in the copy, and diffs the copy with the
  output; any difference fails that gate. The nix check does not run this
  compare, since tauri-build rewrites the copy inside the sandbox. Change the
  commands and regenerate the copy in the same commit.

## Tests

- `tests/window.rs` drives `Window` against `fake-aoide` (from the shell's
  `fakes` feature, path from `AOIDE_DESKTOP_AOIDE`). Its acts are `note add`
  (an unfloored act in `shell/fixtures/schema.json`) and the floored
  `config set`. A change to the fixture's entries moves the shell's argv tests
  too.
- `tests/acl.rs` runs the mock runtime and checks the `main` label and the
  invoke key. Its `smoke_note` tests take each build's side: refused as main
  without `smoke`, and reaching the handler with it.
- `tests/cli.rs` runs the built binary with `--smoke` in a build without the
  feature. It is empty under `smoke`.
- `tests/smoke.rs` (`required-features = ["smoke"]`) runs every file under
  `smoke/` in a headless cage under the dev shell's grim, with the environment
  built from scratch: no DISPLAY, no outside WAYLAND_DISPLAY, no session bus.
  It starts every process through `test_doors::start` and ends cage with
  SIGTERM through `test_doors::terminate`, never a kill. The screenshot goes to
  `AOIDE_SMOKE_SHOTS` when that is set. It fails on a scenario file it cannot
  read or parse. A scenario with `click` has the probe click one answer button
  of the page and holds the fake aoide's argv record to exactly one
  `ask answer` with the scenario's `answered` arguments; the click is the
  smoke build's own probe, never the app's input path.