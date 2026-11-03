# Throws if it is ever read. `notSet` names this file as its `providers`, a path rather than a
# set of targets: habit refuses that value by its type and never reads the file.
throw "thin-flake: a provider path that is not a set was read"
