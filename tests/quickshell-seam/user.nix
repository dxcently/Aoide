# tests/quickshell-seam/user.nix — the fixture person.
#
# A host module attaches this (`habit.users.khoa.definition`), exactly as a real
# host attaches `users/khoa.nix`: its own settings create the account, its
# `habit.home` is the home floor. Nothing here is a fixture of the thing
# under test — the quickshell service writes to the user's home, so a real
# person is what the seam runs against.
{
  users.users.khoa = {
    isNormalUser = true;
    description = "Quickshell seam fixture";
    extraGroups = [ "wheel" ];
    hashedPassword = "!"; # no login — a test account
  };

  habit.home = {
    home.stateVersion = "25.11";
  };
}
