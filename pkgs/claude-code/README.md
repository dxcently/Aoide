# Claude Code

Anthropic's Claude Code CLI, pinned ahead of nixpkgs. The build is nixpkgs'
own `claude-code` recipe; this directory supplies only the release manifest
(`manifest.zst.json`), which carries the version and the per-platform
checksums the fetch verifies against.

The package shadows `pkgs.claude-code` on purpose (`lib/pkgs.nix`
`intentionalShadows`), so the claude-code dendrite and every host `spawnPath`
that names `pkgs.claude-code` get this build.

Update by replacing the manifest with the release's own:

    v=$(curl -fsSL https://downloads.claude.ai/claude-code-releases/latest)
    curl -fsSL "https://downloads.claude.ai/claude-code-releases/$v/manifest.zst.json" \
      -o pkgs/claude-code/manifest.zst.json

Verify with `NIXPKGS_ALLOW_UNFREE=1 nix build .#claude-code --impure --no-link`
before a user-approved rebuild activates it. Once nixpkgs catches up to the
pinned version, delete this directory and its `intentionalShadows` line.
