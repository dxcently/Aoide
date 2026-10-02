# Codex

OpenAI's Codex CLI, repackaged from the official release bundle
(`codex-package-x86_64-unknown-linux-musl.tar.gz`) rather than built from
source as nixpkgs does. The bundle's tree is installed unchanged under
`lib/codex`, because `codex` finds its `codex-resources/` and `codex-path/`
relative to its own binary; `bin/codex` links into it.

The package shadows `pkgs.codex` on purpose (`lib/pkgs.nix`
`intentionalShadows`), so the OpenAI dendrite and any consumer of the overlay
get this build. x86_64-linux only, the one bundle pinned here.

Update by moving `version` to the newest `rust-v*` release
(`https://api.github.com/repos/openai/codex/releases/latest`) and the hash
with it:

    nix hash convert --hash-algo sha256 --to sri $(nix-prefetch-url <bundle url>)

Verify with `nix build .#codex --no-link` before a user-approved rebuild
activates it.
