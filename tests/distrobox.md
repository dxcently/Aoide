# distrobox.md — manual container portability suite

Drops the static core into a real distro userland with no nix present.
Not a gate: it needs network and a container runtime, it is not
reproducible, and upstream images move. `checks.<system>.portability`
(`tests/portability.nix`) already proves the artifact is nix-free and
functional; this suite exists to catch what that hermetic check cannot
imagine — SELinux policy, distro libc quirks, missing `/proc` entries. Run
it by hand when the static build changes meaningfully, not on every commit.

It becomes materially more important once external subcommands (workstream
#138) land: plugins will be shell scripts, and a scratch image has no shell
to run them with. This suite is where that gap would first show.

`nix develop .#testing` provides `distrobox` and `podman`.

```
nix build .#packages.x86_64-linux.aoide-static
BIN=$(readlink -f result)/bin

distrobox create --name aoide-debian --image debian:12
distrobox enter aoide-debian -- true   # first enter provisions the box

distrobox enter aoide-debian -- bash -c "
  mkdir -p ~/bin && cp $BIN/aoide $BIN/aoided ~/bin/
  ~/bin/aoide schema --json | head -c 200
  ~/bin/aoide guide
"

distrobox rm aoide-debian
```

Optionally repeat with `--image fedora:41` (a different libc/SELinux
baseline). Both `schema --json` and `guide` should behave identically to a
NixOS run — no `/nix` mount, no shared libraries pulled in, no error output.
