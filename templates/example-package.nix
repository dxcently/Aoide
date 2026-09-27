# example-package.nix — a package this tree builds itself.
#
# Copy to:  pkgs/<name>/default.nix
# Then:     nothing. Every `pkgs/<name>/default.nix` is discovered and injected
#           into the host overlay, so a lane reaches it as `pkgs.<name>` — by
#           name, never by path. pkgs/ is for builds nixpkgs does not have —
#           not a second copy of nixpkgs; a name that shadows a stock attribute
#           is an error, so if `pkgs.<name>` already exists, use it and delete
#           this file.
# Replace:  <name>, the source, and the hashes. The hashes below are
#           PLACEHOLDERS: build once with `lib.fakeHash`, and Nix prints the
#           real one in the mismatch error.
{
  lib,
  rustPlatform,
  fetchFromGitHub,
}:
rustPlatform.buildRustPackage rec {
  pname = "example-tool";
  version = "0.1.0";

  src = fetchFromGitHub {
    owner = "example";
    repo = "example-tool";
    rev = "v${version}";
    hash = lib.fakeHash; # REPLACE — build once, copy the hash from the error
  };

  cargoHash = lib.fakeHash; # REPLACE — same

  meta = {
    description = "One line about what this builds";
    homepage = "https://example.invalid/example-tool";
    license = lib.licenses.mit;
    mainProgram = "example-tool";
  };
}
