# ChatGPT Linux

Official OpenAI Linux desktop release, repackaged for NixOS. The package
walker exposes `chatgpt-linux` through the root flake and host overlay,
following the Kimi package convention. The OpenAI dendrite installs it.

The versioned Debian archive and SHA-256 are pinned in `default.nix`.
Updates use OpenAI's Debian repository package index; update the version
and hash together. Debian maintainer scripts are not executed.

`chatgpt` launches the bundled Linux runtime. Desktop entries and icons are
installed with the package. Authentication remains interactive.

The bundled Parcel watcher uses the pinned glibc version for libc detection.
This avoids Electron's diagnostic-report CFI trap at `gnu_get_libc_version`.
Node modules are unpacked from ASAR for the patched bundle; native modules
remain available for ELF dependency patching. Application sandboxing stays enabled.
