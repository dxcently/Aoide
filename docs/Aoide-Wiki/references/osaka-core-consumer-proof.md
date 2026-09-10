# Osaka: Aoide-controlled core consumer proof

This proof builds committed Aoide and Lyra binaries on Osaka through a temporary dxflake consumer. Process control, package consumption, host evaluation and activation are separate results. The proposed public module architecture is not implemented by this proof.

## Sources

| Item | Source |
|---|---|
| Aoide base snapshot | `971cad8952faec4bc16156dcbebb9fc93830cdf4` |
| Test-only patch | Two timeout guards + invariant, committed as `e0c6a89` |
| dxflake snapshot | `12c67a17ffe8a3e997552bfd99085f821fc07ea5` |
| Original Aoide pin | `facd6c1c88b952aa0f7275897ca278993a65b8c7` |
| Osaka's existing dxflake HEAD | `e289b600f019502d14a0bb3984d39d0f9b93c5d5` |
| Temporary directory on Osaka | `/tmp/aoide-osaka-core-poc-971cad89` |

Archives contain committed files only. Existing checkouts, uncommitted package/Vesktop edits and lock files are preserved. The build uses an input override with `--no-write-lock-file --no-link`. No activation is performed.

## Control path

```text
Codex on yomi-strix
  -> verified SSH transport to Osaka
     -> aoide spawn: dedicated conducted receiver
     -> aoide send --yes --submit: literal build trigger
        -> bounded Nix build
           -> Aoide PTY log and session record
           -> command, stdout, stderr and actual exit status
```

The receiver accepts only the build trigger, never arbitrary shell text. Attempts run sequentially. Native A2A is not claimed: both node pull and node spawn failed because Osaka expects X-Aoide-Peer while local source uses X-Aoide-Node. SSH reaches the existing local Aoide control interface without changing signature checks or pairing policy.

## Results

The corrected public-package build passed on Osaka, with Nix exit 0. Both built binaries returned their schemas, and the built Lyra validated Sonata livery with no display variables, no checkout at its configured flake root, and no external commands available on PATH.

| Check | Result |
|---|---|
| Local Aoide daemon on Osaka | Active |
| First session | `osaka-core-poc-971cad89`: registered, trigger delivered, Nix exited 1 |
| Host-selected package evaluation | Failed: OpenAI module requires an input absent from dxflake specialArgs |
| Second session | `osaka-core-exports-971cad89`: registered, trigger delivered, compilation completed; Nix check phase failed |
| Corrected session | `osaka-core-fixed-971cad89`: registered, trigger delivered, all package checks passed, exit 0 |
| Lyra standalone smoke | Livery lint passed with isolated state and no external tools on PATH |
| Native A2A | Signing-header incompatibility |
| Activation | Not performed |

The first attempt selects `nixosConfigurations.osaka.pkgs.aoide^out,rice`. Evaluation fails in Aoide's `modules/dendrites/openai.nix` because it unconditionally imports `inputs.chatgpt-desktop-linux.nixosModules.default`. dxflake passes its own inputs to the imported module tree and lacks that attribute.

The second attempt adds one export ONLY to the temporary dxflake copy:

```nix
packages.${system}.aoide = inputs.aoide.packages.${system}.aoide;
```

It selects `packages.x86_64-linux.aoide^out,rice` with the same Aoide source override. This bypasses host module evaluation. Compilation reached the package check phase, where aoide-secrets reported 425 passed and one failed test. The failing test, broker::tests::a_hung_set_template_no_longer_wedges_put_lock_forever, measured 11.077838944 seconds against its five-second bound. Nix exited 1 and produced no accepted package outputs. The corrected attempt adds the existing crate environment mutex to both timeout-mutating broker fixtures before mutation and timing. Independent review approved the patch; assertions and production behavior are unchanged. Both regression tests and the remaining package checks passed. This proves public binary consumption, not a complete reusable Quickshell shell or NixOS deployment.

## Design findings

- Version skew blocks native orchestration before a build can repair deployment; a bootstrap transport remains useful.
- A failed sole-node pull still reports outer status ok, and the roster labels the protocol rejection unreachable; consumers must inspect per-node errors.
- Walking private modules leaks upstream input requirements into consumer specialArgs, even when selecting host packages.
- Public package selection avoids host evaluation, but the outer flake still resolves its input graph; evaluation isolation is not lock-graph isolation.
- A delivered trigger proves input delivery, not build completion; the receiver records the actual Nix exit separately.

## Evidence on Osaka

Under the temporary directory, the first attempt is preserved in `host-evaluation/{command.txt,build.stdout,build.stderr,build.exit}`. The second uses the same filenames at the root. Aoide logs live under `~/.aoide/state/sessions/`, named for each session above.

Local evidence copy: `/tmp/aoide-osaka-proof-evidence/`, including the complete Nix test log and final Aoide graph record. The graph automatically reached `done` when the receiver exited; it does not distinguish build failure from successful completion. During compilation, the initial shell phase was `idle`; an explicit `aoide session phase --phase working` corrected it. Process lifecycle, activity phase and task exit result require separate evidence.

Corrected evidence is copied to `/tmp/aoide-osaka-proof-evidence/corrected/`. Its `build.exit` is 0; `build.stdout` records:

```text
/nix/store/5ivsffwzxf69p4p1lrbc2fk4ay57s441-aoide-0.0.22
/nix/store/s44hsgy84gjr66wj353gnagv2vwrhw1c-aoide-0.0.22-rice
```

These store paths are on Osaka. The corrected receiver also reports working explicitly and enables Nix build logs with `-L`. Aoide's graph reaches done after the process exits. The installed system remains unchanged; the original private-module host-evaluation failure and native A2A version incompatibility remain unresolved.
