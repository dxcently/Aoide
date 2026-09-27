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
- On a painting box, mirror the nix unit: `WantedBy=graphical-session.target`
  with matching `After=`/`PartOf=`. Off a graphical session, `default.target`
  is the anchor — `PartOf=graphical-session.target` there would propagate an
  immediate stop to a manually started daemon.
- If the daemon must outlive logout: `loginctl enable-linger "$USER"`.
- On WSL this shape is reachable as-is — systemd is PID 1 there when
  `/etc/wsl.conf` sets `[boot] systemd=true`.

What a healthy start writes: the audit log line
`{"door":"daemon","command":"daemon","status":"started","message":"aoided resident loop online"}`
in `$AOIDE_ROOT/log`, and a control socket `aoided.sock` plus `events.jsonl` in
`$XDG_RUNTIME_DIR/aoide/` (default `/run/user/<uid>/aoide/`).

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
- `aoided --audit-log <path>` overrides the log directly.
- **Two daemons, one pathname.** A resident `aoided` outlives the shell that
  started it and keeps holding the socket *inode* at that pathname; a second one
  binds the same pathname and the live path silently points at the newer
  process. Before trusting the socket, check for a straggler:
  `ss -xlp | grep aoide` (or `pgrep -a aoided`) — and stop the one you started
  (`systemctl --user stop aoided`) when you are done with it.

## 7. Windows (WSL)

WSL2 is the beta Windows path: it is a Linux box, and everything above runs on
it unchanged. Native Windows is a separate, continuing portability lane and is
not required for beta (`docs/architecture/CORE-POSIX.md`).

```
Windows host
└── WSL2 (Ubuntu)  ← the Linux path above; systemd as PID 1
    ├── clone + target on ext4   (~/Aoide, not /mnt/c/…)
    ├── aoide / aoided in ~/.cargo/bin
    └── dials OUT over ssh  →  another node's loopback door
```

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
- To paint: `README.md` § 4, `docs/BUILD.md` (writing a dendrite or facet),
  [Clone and Run](Aoide-Wiki/concepts/governance/Clone-and-Run.md) (the
  distribution's clone-and-run model and self-update).
