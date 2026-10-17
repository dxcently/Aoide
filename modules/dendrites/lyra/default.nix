# modules/dendrites/lyra — the lyra paint lane: the vibeos runtime (README.md).
# The catalogue entry is this wrapper; the runtime is the plain pair beside it,
# imported by path (AGENTS.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
}
