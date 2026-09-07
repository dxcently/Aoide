---
type: entity
created: 2026-08-25
updated: 2026-08-27
aliases: [lyra binary, lyra command]
tags: [aoide, cli, lyra, quickshell, rice, paint]
---

# lyra (the paint binary)

`lyra` is the AoideOS paint binary: a second composition root over the same
domain-crate handlers `aoide-cli` assembles. In Cordis terms
(`CONTRACTS.md` §0) it is a BUNDLE, same as `cli` — its own ordered
`commands::all()` profile over an independent `Registry`. The crate root is
`pkgs/aoide/crates/lyra/`.

Muse names (`Aoide`, `Melete`, `Mneme`) name systems, not binaries; a binary
inside a system takes an instrument name — the desktop is the instrument
the muse plays.

## What it owns / never owns

Owns the self-ricing loop, `screen`, `herald`, `shellbridge`, `quickshell`
— everything that paints, or that only a desktop needs. Never owns
conducting, the graph, A2A, nodes, or the daemon: those are core `aoide`
identity and ship only in `aoide`/`aoided`. `conductor` stays core for the
same reason — pure Rust (ratatui), no system-closure weight, and
conducting orchestration is Aoide's core identity, not a painting tool.

`lyra` never depends on `aoide-client`/`aoide-conductor` — no A2A client,
no TUI. `run_lyra` drives `aoide_protocol::door::run` with lyra's own
registry and dispatcher, and a smaller `special` hook than core's: `mcp
serve --stdio`, and raw `guide`/`schema`/`livery` output. `a2a serve` and
`conductor` are deliberately absent from it.

Two deliberate charter exceptions carry over from core: `shellbridge.rs`
and `herald.rs` stay as FILES in `aoide-conduct` — only their registry
lines (the CLI commands) move to `lyra` — because both are entangled with
core (`permit.rs` publishes summons through `herald`; `conductor/ui.rs`
reads the socket path `shellbridge` owns).

## The command surface

48 command paths, [[aoide-cli|`aoide`]]'s sibling registry, pinned by its
own golden-snapshot test in `crates/lyra/src/registry.rs`. 47 are real;
`rice transpose` is the one stub (exit 64).

| Group | Leaves | Detail |
|---|---|---|
| `rice` | 18 | `lint`/`stage`/`compose`/`back`/`declare`/`transpose`(stub); the 3-command `draft` group; the 4-command `mode` group; the 5-command `take` rehearsal-snapshot group — [[Rice-and-Livery]] |
| `cover set` | 1 | wallpaper staging — [[Rice-and-Livery]] |
| `livery` | 3 | `lint`/`resolve`/`emit`, the native design-token engine — [[livery]] |
| `element seed` | 1 | renders a song's committed `elements/*/element.json` into `run/elements/` — verbatim copy for `template:false`, livery-rendered for `template:true`, atomic per file — [[Song-Anatomy]] |
| `shellbridge` | 1 | the desktop state bridge daemon — [[Doors-and-Nodes]] |
| `quickshell` | 2 | `reload` (the Quickshell IPC hot-reload trigger) and `healthcheck` (the placeholder-screen lockup watchdog) — [[Meta-and-Upkeep]] |
| `herald push` | 1 | dunst's notification-ledger hook — [[Content-and-Hooks]] |
| `screen` | 14 | capture, OCR, and synthesized-pointer control — [[Screen-Commands]] |
| `guide`, `schema`, `mcp serve` | 3 | the same commands as their `aoide` spellings, over lyra's own registry |
| `onboard` | 1 | the nix half of clone onboarding — generates `./aoide.nix` with every `aoide.*` module option's default commented out, never edits the user's flake — [[Clone-and-Run]] |
| `secrets ask` | 1 | the quickshell code-entry dialog for one parked TOTP ask — six digit boxes, zenity's own output contract on stdout/exit code — [[Secrets-Commands]] |
| `pair` | 2 | `ask` (either direction: collects a typed SAS code read off the far side's screen — the requester's code on an inbound leg, the approver's reply code on an outbound one) and `show` (fires after an inbound commit: displays this instance's own reply SAS large, with Copy and Done and no reject) — the pairing-ceremony popups behind `aoide pair watch --popup`, sharing `secrets ask`'s six-box QML component — [[Pairing-Ceremony]] |

## The nix boundary

`lyra` is the one binary allowed to depend on nix: `song::widgets`'s `nix
eval` call lives here. Core (`aoide`/`aoided`) stays cargo-buildable on any
Linux, zero nix shell-outs, no NixOS assumption — the boundary the root
`AGENTS.md` states as "core is nix-independent."

## Related

- [[aoide-cli]]
- [[livery]]
- [[Package-Layout]]
- [[Rice-and-Livery]]
- [[Screen-Commands]]
- [[Quickshell]]
- [[shellbridge]]
- [[CLI-Reference]]
- [[Overview]]
