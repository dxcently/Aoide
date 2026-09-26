# tests/quickshell-seam/user.nix — the fixture person.
#
# A host record attaches this (`users.khoa.definition`), exactly as a real host
# attaches `users/khoa.nix`: the `nixos` half creates the account, the
# `homeManager` half is the home floor. Nothing here is a fixture of the thing
# under test — the quickshell service writes `home-manager.users.<user>`, so a
# real person is what the seam runs against.
{
  nixos =
    { ... }:
    {
      users.users.khoa = {
        isNormalUser = true;
        description = "Quickshell seam fixture";
        extraGroups = [ "wheel" ];
        hashedPassword = "!"; # no login — a test account
      };
    };

  homeManager = {
    home.stateVersion = "25.11";
  };
}
