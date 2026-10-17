# modules/dendrites/quickshell — the shell lane: the Quickshell package and the
# one `aoide-quickshell` service, on whatever directory `aoide.quickshell.config`
# names (README.md). The catalogue entry is this wrapper; the halves are plain
# modules, imported by path, so a lane that brings the shell imports the same
# two files and lands on one module of each.
{
  imports = [ ./system.nix ];
  habit.home.imports = [ ./home.nix ];
}
