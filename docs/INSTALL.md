# docs/INSTALL.md — installing Aoide

There are two installs and one boundary between them (root `AGENTS.md`,
`docs/architecture/PACKAGE-LAYOUT.md`): **Aoide** is the core — `aoide` and
`aoided`, sessions and the DAG, conduct, the doors — and installs with cargo on
any Linux, no nix. **AoideOS** is the distribution built on that core, and its
paint half (`lyra`, the bar, the dock, the rice loop) ships only through the
nix flake.

| you want | binaries | how it installs |
|---|---|---|
| Aoide (core) | `aoide`, `aoided` | `cargo install` from a checkout — **this page** |
| AoideOS (the desktop) | the same, plus `lyra` | the flake: `README.md` § 4, [Clone and Run](Aoide-Wiki/concepts/governance/Clone-and-Run.md) |

Nothing here installs `lyra`, and nothing here needs nix, a NixOS host, or a
graphical session. `lyra` is a separate crate with the paint dependency weight
(wayland, image decoding, a nix eval) and the flake is its only path.

---

## 1. Prerequisites

Check these; do not blind-install them (a dev box usually has all five).

```sh
rustc --version && cargo --version    # rustup, stable toolchain
cc --version                          # a C toolchain (the linker, not the compiler)
git --version
pkg-config --version
command -v jq                         # optional — reads `schema --json`
```

If a piece is missing:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # rustup (no nix)
# Debian/Ubuntu:  apt-get install -y build-essential git pkg-config
```

`~/.cargo/bin` is where `rustup` installs the toolchain and where
`cargo install` will put the binaries; it must be on `PATH` (`rustup`'s own
shell setup adds it — `command -v cargo` must answer before step 3).

## 2. Get the source

```sh
git clone <upstream-url> ~/Aoide
```

An offline bundle works the same way with one flag:

```sh
git clone -b main <bundle> ~/Aoide
```

`-b main` is required for a bundle, not a preference: a bundle carries no
resolvable HEAD, so a plain `git clone` warns *"remote HEAD refers to
nonexistent ref, unable to checkout"* and leaves an unborn `master` with no
files in it.

## 3. Install the two binaries

```sh
cd ~/Aoide
cargo install --path pkgs/aoide/crates/cli --bins --locked
```

- **`crates/cli` is the app crate.** `pkgs/aoide` is a *virtual* manifest (the
  workspace root, no `[package]`), so `--path pkgs/aoide` fails with
  `error: found a virtual manifest ... instead of a package manifest`.
- **`--bins` installs that crate's two binaries**: `aoide` and `aoided`. There
  is no third: `lyra` lives in `crates/lyra` and is not installed by this.
- **`--locked`** pins the build to the workspace's committed
  `pkgs/aoide/Cargo.lock`.
- The install is a release build — about a minute on a 4-job laptop — and lands
  in `~/.cargo/bin/{aoide,aoided}`.

## 4. First boot — `aoide onboard`

Run it from anywhere inside the checkout (it walks up from the cwd looking for
the repo's own `.claude/skills/aoide/SKILL.md`; outside a checkout it refuses
with `no-checkout` naming that marker):

```sh
cd ~/Aoide && aoide onboard            # ~/.cargo/bin/aoide
```

It is idempotent, and the core half is four acts: register the clone as a graph
project, link `~/song` → `<checkout>/song` (never clobbering an existing file),
seed `song/songbook/preferences.md` when absent, then ask which agent harnesses
to wire and run `hooks install` for each chosen harness.

- `--harness claude,eidolon` wires exactly that list; `--yes` takes every
  harness found on `PATH` and skips every prompt.
- When the `lyra` binary resolves it delegates the nix half to `lyra onboard`;
  on a core-only box it says so and stops — *"lyra: not found ... skipping the
  desktop half; core setup is complete"*. That line is success, not a failure.
- Run it in a directory you own: a session registered from a Windows path under
  WSL lands `(unanchored)` on the graph (see § 6).

## 5. Smoke test

```sh
aoide schema --json | jq -r .aoide    # the version — the only place it lives
aoide soundcheck                      # working-tree sweep: "clean — no findings"
aoide guide                           # the four-tier agent onboarding
```

There is **no `aoide --version`** (unknown flags print the usage block) and
**no `doctor` subcommand**. `schema --json` is the version *and* the machine-readable
command surface; `soundcheck` is the mechanical-integrity check. Together they
are what "is this install healthy" means.

## 6. Start `aoided`

**The declared path is the nix module.** On an AoideOS host,
`pkgs/aoide/module/aoided.nix` writes the systemd **user** unit
(`systemd.user.services.aoided`), and `modules/nucleus/aoided.nix` anchors it to
`aoide.sessionTarget`. Below is the portable equivalent — the same binary, the
same seams, no nix. `aoide daemon` is *not* it: that command runs the one-shot
skeleton self-check (`daemon::run`) and exits, while `aoided` is the resident
tick loop (`daemon::run_loop`) that binds the socket.

### With systemd (a user unit)

`~/.config/systemd/user/aoided.service`:

```ini
[Unit]
Description=Aoide orchestrator daemon
After=default.target

[Service]
ExecStart=%h/.cargo/bin/aoided
Type=simple
Restart=on-failure
RestartSec=5s
Environment=AOIDE_AUDIT_LOG=%h/.aoide/log
Environment=AOIDE_ROOT=%h/.aoide
Environment=AOIDE_FLAKE_ROOT=%h/Aoide
Environment=AOIDE_USER=%u
# Only for `spawn --windowed` / `resurrect`: a user unit inherits no shell
# environment, so the boot auto-resume sweep needs the terminal template here.
#Environment="AOIDE_TERMINAL=kitty -e {cmd}"
NoNewPrivileges=true
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
```

```sh
mkdir -p ~/.aoide/{log,state/stage,song/stage}
systemctl --user daemon-reload
systemctl --user enable --now aoided
systemctl --user status aoided
journalctl --user -u aoided -f
```

- `Type=simple` is correct because the loop never exits on its own;
  `Restart=on-failure` covers a crash or a bind failure without masking one as
  active.
- On a painting box, mirror only the nix unit's ANCHOR:
  `WantedBy=graphical-session.target` with matching
  `After=graphical-session.target`, and **no `PartOf=`**. The anchor decides
  when the daemon starts, never when it stops: a desktop crash (or a logout
  that stops the shell) must not take the audit log, the gate and the doors
  down with it, and a stop systemd propagates is a deliberate one that
  `Restart=` will not undo. What surviving costs is stated plainly — the
  process that outlives a desktop crash keeps the dead session's
  `WAYLAND_DISPLAY`/`HYPRLAND_INSTANCE_SIGNATURE`, so anything it launches
  into the desktop can aim at a stale socket until the next
  `systemctl --user restart aoided`. Off a graphical session,
  `default.target` is the anchor and none of this applies.
- If the daemon must outlive logout: `loginctl enable-linger "$USER"`.
- On WSL this shape is reachable as-is — systemd is PID 1 there when
  `/etc/wsl.conf` sets `[boot] systemd=true`.

What a healthy start writes: the audit log line
`{"door":"daemon","command":"daemon","status":"started","message":"aoided resident loop online"}`
in `$AOIDE_ROOT/log`, and a control socket `aoided.sock` plus `events.jsonl` in
`$XDG_RUNTIME_DIR/aoide/` (default `/run/user/<uid>/aoide/`).

### The door beside it (a user unit)

`aoided` binds no TCP port: the A2A door is its own process, exactly as
`pkgs/aoide/module/aoided.nix` declares them as two units. The portable
equivalent, beside the unit above —
`~/.config/systemd/user/aoide-a2a.service`:

```ini
[Unit]
Description=Aoide A2A (Agent2Agent) door (loopback by default, user-only)
After=aoided.service
BindsTo=aoided.service

[Service]
Type=simple
ExecStart=%h/.cargo/bin/aoide a2a serve
Restart=on-failure
RestartSec=5s
Environment=AOIDE_A2A_BIND=127.0.0.1
Environment=AOIDE_A2A_PORT=8710
Environment=AOIDE_AUDIT_LOG=%h/.aoide/log
Environment=AOIDE_USER=%u
Environment=AOIDE_ROOT=%h/.aoide
NoNewPrivileges=true
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=aoided.service
```

```sh
systemctl --user daemon-reload
systemctl --user enable --now aoide-a2a
systemctl --user is-active aoide-a2a
curl -sS -o /dev/null -w '%{http_code}\n' \
  http://127.0.0.1:8710/.well-known/agent-card.json     # 200
```

- `WantedBy=aoided.service` is the nix module's own `wantedBy`; `BindsTo` +
  `After` are its `bindsTo`/`after` — the door starts with the daemon and goes
  down with it, never the other way round. Enabling writes the symlink under
  `aoided.service.wants/`, so the door is never a second thing to remember.
- The door is loopback-only by construction; `AOIDE_A2A_BIND` exists for an
  operator's deliberate choice, not as a default.
- `AOIDE_FLAKE_ROOT` is song/paint data — a host that has a checkout to rice
  sets it (the WSL twin in § 7.3 does); core's door runs without it.
- On a host running two nodes (native + WSL, § 7.3), this unit is where one
  node's identity and port differ from the other's.

### The mail adapter beside it (a relay only)

A relay that faces a TLS-terminating front runs `aoide mail serve` as a third
unit, `~/.config/systemd/user/aoide-mail-adapter.service`. It is the A2A
door's unit with two changes: the command, and the port variable. There is no
bind variable, because the adapter binds `127.0.0.1` and nothing else.

```ini
[Unit]
Description=Aoide mail adapter (H1: loopback, mail-only, behind a TLS-terminating front)
After=aoided.service
BindsTo=aoided.service

[Service]
Type=simple
ExecStart=%h/.cargo/bin/aoide mail serve
Restart=on-failure
RestartSec=5s
Environment=AOIDE_MAIL_ADAPTER_PORT=8712
Environment=AOIDE_AUDIT_LOG=%h/.aoide/log
Environment=AOIDE_USER=%u
Environment=AOIDE_ROOT=%h/.aoide
NoNewPrivileges=true
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=aoided.service
```

```sh
systemctl --user daemon-reload
systemctl --user enable --now aoide-mail-adapter
systemctl --user is-active aoide-mail-adapter
curl -sS http://127.0.0.1:8712/.well-known/agent-card.json     # name, protocolVersion, url
```

The front's ingress targets `127.0.0.1:8712`, never the door's `8710`.
`openssh` and `curl` must resolve on the unit's `PATH`: a transit deposit
drains toward its next hop from inside this process.

### Without systemd

There is no supervisor to write the unit into, so run the same binary under
whatever one you have — a foreground terminal, `supervisord`, a container
`CMD`, `tini` — with the same environment:

```sh
export AOIDE_ROOT="$HOME/.aoide" AOIDE_FLAKE_ROOT="$HOME/Aoide" AOIDE_USER="$USER"
export AOIDE_AUDIT_LOG="$AOIDE_ROOT/log"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/aoide-$UID}"   # short: a unix socket path has a ceiling
mkdir -p "$XDG_RUNTIME_DIR" "$AOIDE_ROOT"/{log,state/stage,song/stage}
aoided
```

- Without `XDG_RUNTIME_DIR` the resolver falls back to `/run/user/<uid>`, which
  a box with no logind has not created — set it, or point
  `AOIDE_DAEMON_SOCKET` (and `AOIDE_DAEMON_EVENTS`) somewhere short and private.
- `aoided --audit-log <path>` overrides the log directly. `aoided --version` and
  `aoided --help` answer and exit; any other argument is refused (exit 2) and
  starts nothing, so probing a deployed box never launches a second daemon.
- **Two daemons, one pathname.** A resident `aoided` outlives the shell that
  started it and keeps holding the socket *inode* at that pathname; a second one
  binds the same pathname and the live path silently points at the newer
  process. Before trusting the socket, check for a straggler:
  `ss -xlp | grep aoide` (or `pgrep -a aoided`) — and stop the one you started
  (`systemctl --user stop aoided`) when you are done with it.
- **A daemon started as a background job IGNORES `SIGINT`.** The shell sets
  `SIGINT`/`SIGQUIT` to *ignored* for an asynchronous job in a non-interactive
  shell (`sh -c 'aoided &'`, `setsid aoided &`), so `kill -INT <pid>` is a
  silent no-op — it returns 0 and nothing happens. Use `SIGTERM`, which is what
  systemd sends and what the unit's `Restart=on-failure` is written around.
  Measured on a hand-started `aoided`: `/proc/<pid>/status` had the `SIGINT` bit
  set in `SigIgn`, and `kill -TERM` ended it cleanly where `kill -INT` had not.
  If a hand-started daemon has to go, that is the honest order: TERM it, watch
  it leave, *then* let the unit start its own.

## 7. Windows

Windows runs the core two ways, and when both run at once they are **two
nodes**, not one node in two places: native Windows is a primary target
(root `AGENTS.md`, `docs/architecture/CORE-POSIX.md`), WSL2 is the beta Linux
path beside it.

| you want | how |
|---|---|
| the core, native — `aoide`, `aoided`, no WSL/MSYS/Cygwin | § 7.1, then start it at logon (§ 7.2) |
| the Linux path, and the AoideOS beta | § 7.3 — WSL2, everything above unchanged |

### 7.1 Native Windows (core)

Prerequisites are § 1's, on the host's own toolchain: rustup with
`x86_64-pc-windows-msvc` (the MSVC linker `link.exe`; no MinGW, no MSYS), `git`,
and a shell. Then § 3–§ 5 verbatim — `cargo install --path
pkgs/aoide/crates/cli --bins --locked` lands
`%USERPROFILE%\.cargo\bin\{aoide,aoided}.exe`, `aoide onboard` is idempotent the
same way, and `aoide schema --json` is the version. Two things differ.

**Paths.** There is no `/run/user/<uid>` and Windows does not set `$HOME`: the
runtime dir (the sockets) defaults to `%LOCALAPPDATA%`, the user's home to
`%USERPROFILE%`, and therefore the root to `%USERPROFILE%\.aoide` — the shape
`~/.aoide` has on Linux. `AOIDE_ROOT`, `AOIDE_AUDIT_LOG`, `AOIDE_STAGE_DIR`,
`AOIDE_STATE_DIR` and `XDG_RUNTIME_DIR` are overrides here too, so a check that
must not touch live state sets all five.

**One daemon, and its doors are other processes.** `aoided` binds a unix socket
and **no TCP port**; `aoide a2a serve` (the door) and `aoide mcp serve --stdio`
are separate processes, exactly as `aoide-a2a`/`aoide-mcp` are separate units on
Linux. `aoided` spawns neither.

### 7.2 Start `aoided` at logon — a per-user scheduled task

The declared path on an AoideOS host is still the nix module
(`pkgs/aoide/module/aoided.nix`); § 6 is the portable Linux equivalent. This is
the portable Windows one: **per-user environment variables for the process's
environment, and per-user logon tasks for its lifetime** — the OS's own
equivalent of the unit's `Environment=` lines and its `[Install]` anchor.

```powershell
# 1. The environment the daemon and the door run with. User scope, because it
#    must be there at every logon, for every process, with no shell involved.
[Environment]::SetEnvironmentVariable('AOIDE_ROOT',      "$env:USERPROFILE\.aoide", 'User')
[Environment]::SetEnvironmentVariable('AOIDE_AUDIT_LOG', "$env:USERPROFILE\.aoide\log", 'User')
[Environment]::SetEnvironmentVariable('AOIDE_A2A_BIND',  '127.0.0.1', 'User')   # doors stay loopback-only
[Environment]::SetEnvironmentVariable('AOIDE_A2A_NODE_NAME', 'thinkchiyo-win', 'User')  # a second node on one box needs its own name
[Environment]::SetEnvironmentVariable('AOIDE_A2A_PORT',  '8720', 'User')        # ... and its own port
[Environment]::SetEnvironmentVariable('AOIDE_MAIL_ADAPTER_PORT', '8722', 'User')

# 2. Two tasks: the daemon, and the door beside it.
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
  -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 `
  -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$exe     = "$env:USERPROFILE\.cargo\bin"
Register-ScheduledTask -TaskName AoideAoided -Force -User $env:USERNAME -RunLevel Limited `
  -Trigger $trigger -Settings $settings -Action (New-ScheduledTaskAction -Execute "$exe\aoided.exe")
Register-ScheduledTask -TaskName AoideA2a -Force -User $env:USERNAME -RunLevel Limited `
  -Trigger $trigger -Settings $settings -Action (New-ScheduledTaskAction -Execute "$exe\aoide.exe" -Argument 'a2a serve')

# 3. Start them now, without logging out. (`Start-ScheduledTask`'s -TaskName is
#    a single string, unlike Get-ScheduledTask's: pipe the tasks to it.)
Get-ScheduledTask -TaskName AoideAoided, AoideA2a | Start-ScheduledTask

# 4. Check.
Get-ScheduledTask AoideAoided, AoideA2a | Get-ScheduledTaskInfo |
  Select-Object TaskName, LastRunTime, LastTaskResult
Get-NetTCPConnection -State Listen -LocalPort 8720
Invoke-WebRequest -UseBasicParsing http://127.0.0.1:8720/.well-known/agent-card.json |
  Select-Object -ExpandProperty Content
```

- **`-ExecutionTimeLimit ([TimeSpan]::Zero)` is load-bearing.** The default is
  three days, which would stop a resident daemon and restart it — a supervisor
  that kills its own subject on a timer. Zero means "no limit".
- **`-RunLevel Limited`** keeps the task unelevated: it runs as this user with
  this user's own token, which is what the loopback-only door and the
  audit log's ownership assume.
- **The action carries a PATH, never an environment.** Task Scheduler hands the
  process the user's own environment, which is why step 1 writes persistent user
  variables; a value that must not be persistent belongs in a wrapper instead
  (`-Execute cmd.exe -Argument '/c set AOIDE_X=… && aoided.exe'`), and a
  wrapper is the honest choice only when the value is per-run.
- **`-RestartCount`/`-RestartInterval`** are this host's `Restart=on-failure` /
  `RestartSec=5s`. A clean exit under Task Scheduler is a task that has ended,
  so the daemon's "never exits on its own" is what keeps the task active — the
  same reason the unit is `Type=simple`.
- **Lifetime, stated rather than implied.** A logon task's process runs in that
  logon's session, so it starts at logon and ends when the session does —
  the opposite of § 6's anchor note, where deliberately *no* `PartOf=` ties the
  daemon to the desktop. A crash comes back in a minute; a logoff comes back at
  the next logon. Running a daemon that outlives every logon on this host means
  a service in session 0 with an S4U/service account, which is a different
  install, not a flag here.
- **A daemon started inside an ssh session is not resident.** Measured on
  ThinkChiyo: `Start-Process ... aoided.exe` over ssh was alive at 60 s and gone
  by the next session, with an empty stderr and no stop record. That is why the
  task, not the ssh command, is the mechanism.
- **The inverse is one command**: `Unregister-ScheduledTask -TaskName
  AoideAoided, AoideA2a -Confirm:$false`, plus the same
  `SetEnvironmentVariable(..., $null, 'User')` lines. Nothing else in the tree
  knows these tasks exist.
- **Two daemons, one pathname** — § 6's warning, in this host's spelling: a
  resident `aoided` holds the socket inode at `<runtime dir>\aoide\aoided.sock`,
  and a second one binds the same pathname, so before trusting the socket check
  for a straggler (`Get-Process aoided`; `Get-ChildItem
  $env:LOCALAPPDATA\aoide`) and stop the one you started.

### 7.3 WSL2 — the Linux path

WSL2 is a Linux box, and everything above runs on it unchanged.

```
Windows host
└── WSL2 (Ubuntu)  ← the Linux path above; systemd as PID 1
    ├── clone + target on ext4   (~/Aoide, not /mnt/c/…)
    ├── aoide / aoided in ~/.cargo/bin
    └── dials OUT over ssh  →  another node's loopback door
```

**Two nodes on one box.** With § 7.1 running natively, the Windows node and the
WSL node are separate nodes with separate records, and three collisions are
already waiting for them:

- **The name.** Both compute their node name from the same computer name (WSL
  takes the Windows host name), so one side must set `AOIDE_A2A_NODE_NAME` — the
  override the whole chain reads — or the two nodes' records, letters and
  self-checks cannot tell each other apart.
- **The port.** WSL2 forwards the VM's `127.0.0.1` listeners to the Windows
  host's `127.0.0.1`, so a door on the same port on both sides contends for one
  address. Give them different ports (§ 7.2's `8720`/`8722` for the native side,
  `8710`/`8712` left to WSL).
- **Direction.** Windows can dial the WSL door straight through that forwarding
  (`aoide pair http://127.0.0.1:8710/`); the other way round the VM's
  `127.0.0.1` is the VM's own loopback, so a WSL-side dial needs an ssh hop by
  address. The one-box path is therefore: **Windows is the requester.**
- **The name has to be the peer's own, at pair time.** `pair <url>` nicknames
  the far side after the URL's sanitized host (`http://127.0.0.1:8710/` →
  `127-0-0-1`), and that nickname is what this side then writes into a letter's
  address — the receiving node refuses a container addressed to anyone but the
  name it knows itself by:

  ```
  refused: addressing-mismatch: container is addressed to `127-0-0-1`, not this node
  ```

  So a URL pair against a node that calls itself `thinkchiyo` passes
  `--name thinkchiyo` at the ceremony. Afterwards it is a re-pair: a node
  record's name *is* the address, and no command renames one.
- **Grants are the mesh's, and a pair stamps the default.** The pair above lands
  whatever `[pairing] defaultGrant` names (`read` on a stock config) in the mesh
  both ends agreed (unset here, so `home`). Mail needs `message`, and the
  deposit refusal teaches the fix rather than dropping the letter — on the
  *receiving* host `aoide node allow <peer> message on --mesh <mesh>`, then on
  the sender `aoide mail outbox retry --refused`.
- **Upgrade every node together: the wire is one-way across versions.** From
  `0.0.26` on, a signed request carries the mesh it acts in as a sixth field of
  the bytes its signature covers. A door older than that rebuilds five fields,
  so every signed command from the upgraded node to the un-upgraded one is
  refused `-32007` `signature verification failed` while the reverse still
  works, and a pair across the two versions succeeds and looks healthy in
  `node list` while reaching only one way. No fallback exists, deliberately:
  accepting both encodings would let whoever can answer with the refusal strip
  the mesh from the signed bytes. Pair, `node pull`, `send`, `node spawn`, mail
  deposit and mail poll are the ones that bite; the ceremony is not. The same
  release also replaces `state/nodes.json`'s `allows` with per-mesh `grants`,
  and an older binary's first write of that file erases them — so a rollback
  keeps the file, or re-grants from the charter.
  `docs/Aoide-Wiki/concepts/orchestration/Node-Federation.md` has the table.
- **The install on this side is § 6's pair of units.** `aoided.service` plus its
  `aoide-a2a.service` twin, `enable --now`, the door on `8710`, this box's own
  node name unchanged — what distinguishes two nodes on one box is their names,
  ports and grants, never the box.

- **Keep the clone and `target/` on ext4, never `/mnt/c`.** `/mnt/c` is 9p
  (`type=v9fs`), measured at 169 MB/s fsync write against 1.3 GB/s on ext4, and
  slower again on the fork-heavy scan a build does. Use `/mnt/c` only to move
  files in and out.
- **Conduct sees WSL terminals only.** WSLg sets `DISPLAY`/`WAYLAND_DISPLAY`
  but there is no compositor: no `HYPRLAND_INSTANCE_SIGNATURE`, no `hyprctl`, so
  window discovery yields `windowAddress: ""` and `AOIDE_TERMINAL` is unset —
  `spawn --windowed` and `resurrect` answer the taught no-terminal error.
  Headless spawning, registration, sealing and `send` all work; conducting
  reaches WSL sessions, never Windows ones.
- **WSL dials out; nothing dials in.** WSL2 is behind NAT, so a node on the LAN
  cannot reach it without a Windows-side change — and a door bound to loopback
  is not reachable from outside its own box by design. Pair outward, through
  the far node's own loopback, over ssh:

  ```sh
  aoide pair http://127.0.0.1:8710/ --via ssh://<user>@<host>
  ```

  This box's public key must be in that host's `~/.ssh/authorized_keys` — aoide
  never writes it, and the refusal says exactly that. The return leg needs no
  Windows port-forward: the far node spools its letters, and this side collects
  them by asking —

  ```sh
  aoide mail poll        # poll every paired node this box holds `message` for
  ```

- **Windows `PATH` is appended to WSL's `PATH`.** So `aoide onboard`'s
  "every harness found on `PATH`" can find and wire a *Windows* binary
  (`/mnt/c/.../eidolon.exe`), and `wsl -e sh -c` starts in the Windows cwd.
  Both are interop, not installation:
  check `command -v <harness>` before trusting what a wire-up found, pass
  `--harness` to name exactly what to wire, and run the session commands from a
  Linux-side directory (a DrvFs cwd registers as-is and lands the session
  `(unanchored)` on the graph).
- Two smaller edges, both measured: the WSL clock agreed with the host to the
  second (pairing's SAS and time windows are safe), and `sh` under
  `wsl -e sh -c` is `dash` — no process substitution. For scripted setup over
  `ssh` → `cmd.exe` → `wsl` → `sh`, pipe a quoted script to `sh` on stdin rather
  than nesting quotes. A fresh WSL user is in the `sudo` group but may have no
  passwordless `sudo`.

## 8. Where to go next

- `aoide guide` — the four-tier map, printed by the binary itself.
- Root `AGENTS.md` + `docs/agent/README.md` — the same orientation for a
  session with the checkout in view; `docs/Aoide-Wiki/ingest/index.md` is the
  wiki's own table of contents.
- `docs/architecture/CORE-POSIX.md` — what of the core is portable, per
  capability, and what is a taught refusal.
- To paint: `README.md` § 4, `docs/BUILD.md` (writing a dendrite or a paint lane),
  [Clone and Run](Aoide-Wiki/concepts/governance/Clone-and-Run.md) (the
  distribution's clone-and-run model and self-update).
