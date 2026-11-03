# lib/by-class.nix — the `{ byClass = …; }` registry marker, rendered for habit.
#
# A catalogue entry may be the DATA marker `{ byClass = X; }`, and nothing else in
# its attrset. X is a file (or a directory's default.nix), or the record inline. The
# record's keys are Nix module classes: `nixos`, and `homeManager` (the class Home
# Manager's own modules declare). The record is one dendrite written as a NixOS module
# and a Home Manager module in one place. At least one of the two keys is present, and
# no other: a `darwin` key is refused. Each key holds any module habit takes (a path,
# an attrset or a function). The marker mirrors habit's own `{ providers = …; }` data,
# so the registry stays plain data.
#
# Habit imports a catalogue path only when selection keeps it, and reads a value
# only when it is selected too. The marker follows the same rule: `renderCatalogue`
# turns each one into a FUNCTION module before the merged registry reaches habit.
# A function is a value habit takes as it is, and the module system applies it only
# when a host selects the entry. Applying it imports X, checks it, and returns a module
# with `_file` set. That module imports X's `nixos` module when there is one, and sets
# `habit.home` to X's `homeManager` module when there is one. Habit's lanes split it:
# `imports` applies on the system, and `habit.home` reaches each user's Home Manager.
# Without a `nixos` key `imports` is `[ ]`, and without a `homeManager` key `habit.home`
# is absent (habit reads the key's presence).
#
# A marker stands in one of two places, and only in these: a catalogue line
# (`name = { byClass = X; };`) or a provider target of an inline provider set
# (`name = { providers.<p> = { byClass = X; }; };`). A provider-set FILE is left as
# it is. Finding a marker inside one would import that file at catalogue time, for
# every host, and habit imports such a file only when a host selects it.
{ lib }:
let
  # `{ byClass = X; }` and nothing else. An entry with another attribute beside
  # `byClass` is not rendered: habit passes it on as a module, and the NixOS module
  # system refuses that attribute by name.
  isMarker = entry: builtins.isAttrs entry && builtins.attrNames entry == [ "byClass" ];

  # An inline provider set: `{ providers = { <p> = <target>; }; }`.
  isProviderSet = entry: builtins.isAttrs entry && entry ? providers;

  classNames = [
    "nixos"
    "homeManager"
  ];

  # The function module for one marker. `label` names an inline record, which has
  # no file: the catalogue name, or `<name>.<provider>` for a provider's target.
  byClassModule =
    label: byClass:
    let
      isFile = builtins.isPath byClass;
      where =
        if isFile then
          toString (if lib.pathIsDirectory byClass then byClass + "/default.nix" else byClass)
        else
          "catalogue entry '${label}' (inline byClass)";
    in
    _:
    let
      value = if isFile then import byClass else byClass;
      refuse =
        found:
        throw "${where} is not a byClass record: found ${found}. A byClass record is { nixos = <module>; homeManager = <module>; }, with at least one of the two.";
      extra = lib.subtractLists classNames (builtins.attrNames value);
      checked =
        if !builtins.isAttrs value then
          refuse "a value of type ${builtins.typeOf value}"
        else if extra != [ ] then
          refuse "key(s) ${lib.concatStringsSep ", " extra}"
        else if builtins.attrNames value == [ ] then
          refuse "no keys"
        else
          value;
    in
    {
      _file = where;
      imports = lib.optional (checked ? nixos) checked.nixos;
    }
    // lib.optionalAttrs (checked ? homeManager) { habit.home = checked.homeManager; };

  renderCatalogue =
    catalogue:
    lib.mapAttrs (
      name: entry:
      if isMarker entry then
        byClassModule name entry.byClass
      else if isProviderSet entry then
        entry
        // {
          providers =
            if builtins.isAttrs entry.providers then
              lib.mapAttrs (
                provider: target:
                if isMarker target then byClassModule "${name}.${provider}" target.byClass else target
              ) entry.providers
            else
              entry.providers;
        }
      else
        entry
    ) catalogue;
in
{
  inherit renderCatalogue;
}
