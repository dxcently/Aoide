# Content, Herald & Hooks Verbs

Three surfaces in one page: the `content` verb group (the [[Content-Pipeline]]
admission/index flow — all five are exit-64 stubs today), `herald push`
(dunst's `script` hook feeding the notification ledger the Quickshell herald
draws), and `hooks install` (wiring an agent harness's settings file into the
[[Agent-Hooking]] door). Handlers and registrations live in
`pkgs/aoide/crates/conduct/src/commands/{herald,hooks}.rs`,
`pkgs/aoide/crates/conduct/src/herald.rs`, and (content stubs)
`pkgs/aoide/crates/cli/src/commands/stubs.rs`.

Every command takes `--json`. Without it the CLI prints the human `message`
line; with it, an envelope `{status, command, message, gated, changed?, data?}`
(`pkgs/aoide/crates/protocol/src/output.rs`). Exit codes: 0 ok, 1 error,
2 usage, 64 not-implemented.

### aoide content register

```
aoide content register <path> [--json]
```

- **Notes:** **stub** (`implemented: false` in
  `pkgs/aoide/crates/cli/src/commands/stubs.rs::register_content`) — arg
  parsing and schema are real; `dispatch()` short-circuits to the structured
  not-implemented envelope, exit 64. Contract surface (schema summary +
  [[Content-Pipeline]]): registers a content source folder by pointing at it
  in place — never copies; `<path>` must hold `.aoide/manifest.toml`. The
  on-disk registry location is not decided in source (no handler exists) —
  unverified. Not gated.

### aoide content propose

```
aoide content propose <path> [--json]
```

- **Notes:** **stub**, exit 64 (same registration as above). Contract surface:
  proposes a discovered source folder for admission through the approve gate —
  discovery finds, the user admits. Not gated.

### aoide content approve

```
aoide content approve <path> [--json]
```

- **Notes:** **stub**, exit 64. `gated: true` in the schema — the
  non-negotiable user gate of [[Content-Pipeline]] (an ungated agentic index
  would be an injection surface). The wiring of the gate itself is not
  implemented yet.

### aoide content ingest

```
aoide content ingest [<path>] [--json]
```

- **Notes:** **stub**, exit 64. Contract surface: indexes an approved source
  in place, then lints; a lint failure quarantines the content rather than
  discarding it, so drift re-lints and never poisons the live index. No
  `<path>` ingests all approved sources. No quarantine directory path exists
  in source — unverified. Not gated.

### aoide content query

```
aoide content query <query> [--limit <n>] [--json]
```

- **Notes:** **stub**, exit 64. Contract surface: queries the content index;
  `--limit` caps the result count. Not gated.

### aoide herald push

```
aoide herald push [--json]
```

Machine-facing only — dunst runs it per notification as its `script` hook
(`modules/dendrites/dunst.nix`, every rule sets `skip_display`); a human never
types it. To *send* a notification use `notify-send`.

- **Reads:** the `DUNST_*` environment dunst sets: `DUNST_ID`, `DUNST_APP_NAME`,
  `DUNST_SUMMARY`, `DUNST_BODY`, `DUNST_ICON_PATH`, `DUNST_URGENCY`
  (uppercase; lowered on the way in, unknown → `normal`), `DUNST_PROGRESS`
  (`-1` = no value, else clamped to 0–100), `DUNST_CATEGORY`,
  `DUNST_STACK_TAG`, `DUNST_TIMEOUT`. `DUNST_TIMESTAMP` is deliberately
  ignored (monotonic, not wall clock) — the record gets its own `receivedAt`
  from `aoide_storage::time::now_iso_utc()`. dunst's five trailing positional
  args are ignored. There is no `DUNST_ACTIONS`; sender text is carried
  verbatim as data, never parsed or interpreted.
- **Writes:** none directly. Sends one newline-delimited JSON line
  `{"cmd":"heraldpush","notification":{…}}` over the shellbridge unix socket
  `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`
  (`pkgs/aoide/crates/conduct/src/shellbridge.rs::send_line`). The bridge
  daemon — the single writer, serialising concurrent pushes through its accept
  loop — folds the record into the ledger `song/stage/herald.json` (resolves
  `~/Aoide/song/stage/herald.json`; `$AOIDE_STAGE_DIR` absolute override),
  written atomically by the daemon (`edit_ledger`). Ledger rules
  (`pkgs/aoide/crates/conduct/src/herald.rs::apply_push`): a non-empty
  `stackTag` replaces the entry holding the same tag, a repeated `id` replaces
  too, everything else appends newest-last, oldest fall off the front at
  `LEDGER_CAP = 20`. The QML herald reads `herald.json`; `graph permit`
  publishes permission summonses (`kind: "summons"`) into the same ledger.
- **Output:** pushed → `"pushed `<id>`"`, data `{pushed: true, id, urgency,
  progress}`. Empty record (no summary and no body) → ok
  `"empty notification ignored"`, data `{pushed: false, reason: "empty"}`.
  Socket unreachable → error, exit 1, `"could not reach the shellbridge: …"`,
  data `{pushed: false, id}`.
- **Notes:** not gated. No flags beyond `--json`.

### aoide hooks install

```
aoide hooks install <agent> [--capture] [--json]
```

Idempotent merge of aoide's hook wiring into an agent harness's settings file
so its hook stream pipes into `aoide graph session hook`. `<agent>` is one of
`claude | kimi | pi`; the per-harness path/format lives in
`pkgs/aoide/crates/protocol/src/agents.rs` (`SettingsSpec`).

- **Reads:** the existing settings file (must not be clobbered):
  - claude: `$HOME/.claude/settings.json` (JSON — parsed, merged, reserialized;
    an invalid-JSON file is an **error**, never a rewrite).
  - kimi: `$KIMI_CODE_HOME/config.toml` when the env var is set and non-empty,
    else `$HOME/.kimi-code/config.toml` (TOML — text-level append of
    `[[hooks]]` blocks at EOF; never parse-rewritten, the file holds
    providers/credentials).
  - `$HOME` (and `KIMI_CODE_HOME` for kimi) are the only env vars read.
- **Writes:** the same settings file, only when at least one event was added
  (a fully-wired file stays byte-identical); parent dirs are created, a
  missing file is created. Direct `std::fs::write` — **not** an atomic
  temp-then-rename. With `--capture`: also creates `~/Aoide/state/` so the tee
  target dir exists (the capture log itself, `~/Aoide/state/<agent>-hooks.jsonl`,
  is written later by the installed `tee`, not by this command).
- **Output:** changed → `"installed N hook(s) for <agent> → <path>"`, data
  `{agent, settings, added: [...], present: [...], capture, changed: true}`;
  unchanged → `"all N hooks already installed for <agent> (<path>)"`,
  `changed: false`. pi → ok with data `{reason: "declarative", settings:
  ".pi/agent/extensions/aoide-pi-session.ts", changed: false}` — its wiring is
  a declarative file the NixOS dendrite manages, so nothing is written.
  Unknown agent → error, exit 1, data `{reason: "unknown-agent", agent,
  known}`; missing `$HOME` → `{reason: "no-settings-path"}`; write/parse
  failure → `{reason: "settings-unwritable", settings}`.
- **Notes:** not gated. Wired events: the nine core events both profiles map
  (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`,
  `SubagentStart`, `SubagentStop`, `SessionEnd`, `Notification`), plus kimi's
  dedicated `PermissionRequest` (10 for kimi, 9 for claude). Hook commands
  installed:
  - claude (plain): `a=$(command -v aoide) || exit 0; "$a" graph session hook
    >/dev/null 2>&1; exit 0` — a missing aoide is a silent no-op.
  - kimi (plain): `aoide graph session hook --agent kimi`, as a `[[hooks]]`
    block carrying only `event`/`command`/`timeout = 5` (extra fields break
    kimi's config load).
  - `--capture` (either harness): `sh -c 'tee -a
    "$HOME/Aoide/state/<agent>-hooks.jsonl" | aoide graph session hook --agent
    <agent>'`. Temporary debugging only; the `-hooks.jsonl` marker is a
    distinct idempotency key, so capture entries coexist with plain ones
    (installing without `--capture` replaces nothing) and are removed
    manually. `~/Aoide/log` is the audit log file, not this capture path.
  - Idempotency: an event counts as installed when an existing entry's command
    contains `graph session hook` and matches the current mode's capture
    marker; re-running either mode adds nothing.

## Related

- [[Content-Pipeline]]
- [[Agent-Hooking]]
- [[Mneme]]
- [[Rebuild-Gate]]
- [[aoide-cli]]
