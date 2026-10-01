# Claude Code package invariants

- The manifest is upstream's file, fetched whole; never hand-edit a checksum.
- Build through nixpkgs' `package.nix`; do not fork its recipe here.
- Keep the auto-updater disabled (nixpkgs' wrapper does this); updates are pins.
- Update README.md when the update path or the shadow changes.
