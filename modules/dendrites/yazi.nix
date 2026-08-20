# modules/dendrites/yazi.nix — the yazi terminal file manager.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.yazi.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# yazi.nix): programs.yazi with the dxflake layout/sort settings, a `y` shell
# wrapper (bash integration; the bash dendrite also defines a `y` cd-wrapper —
# they name the same tool), and a `sy` = "sudo yazi" alias.
#
# opener/open rules (NOT in the dxflake source — dxflake never had these
# either, so this is new, not restored): the ported config carried layout and
# theme but no way to actually open a file. `O` (open --interactive) had
# nothing to offer a choice between.
#
# Yazi 26.5.6 compiles its default opener/open config into the binary and
# ships no config files in its package — `man 5 yazi-config` CONFIGURATION
# MIXING confirms a bare `rules = [...]` under [open] "rewrite[s] the entire
# default rules", while `prepend_rules`/`append_rules` merge onto the
# defaults instead (same mechanism the man page shows for keymap.toml). Using
# bare `rules` here would have silently deleted yazi's built-in fallback
# behaviour for every file type this dendrite doesn't name — worse than the
# current no-opener state, not better — so prepend/append is load-bearing,
# not a style choice. `[opener]` itself is a plain name -> command-list table
# (not an ordered list), so unlike keymap/open it merges by key: naming
# `edit`/`open` here only defines those two names and leaves whatever else
# yazi compiled in untouched, whichever way the merge is implemented.
#
# Openers are restricted to binaries this dendrite can see are actually
# installed on every host that turns yazi on: `nvim` (modules/dendrites/
# neovim.nix, aoide.neovim.enable, mkDefault true in hosts/common) and
# `xdg-open` (modules/dendrites/cli.nix ships xdg-utils, aoide.cli.enable,
# also mkDefault true). No media-player dendrite exists in this repo yet, so
# there is deliberately no mpv/imv/feh opener — xdg-open is the fallback for
# everything that isn't plain text, per the dendrite discipline (no reading
# another module's options, so this can't check what a host actually has a
# desktop handler for; xdg-open just defers that decision to XDG).
#
# Rule set is deliberately small (YAGNI): text gets a real choice between
# `edit` and `open` (the only place `O`'s interactive picker has more than
# one thing to offer); everything else — images, video/audio, archives —
# gets `open` explicitly named so it doesn't depend on whatever yazi's own
# compiled default rule for that type happens to reference; a trailing
# `url = "*"` in append_rules is the safety net for anything not named
# above. The rule key is `url` (a path glob), NOT `name` — yazi 26.x renamed
# it, and a `name = "*"` rule makes the WHOLE yazi.toml fail to parse ("at
# least one of `url` or `mime` must be specified"), silently dropping every
# setting in the file back to yazi's compiled presets.
{ config, lib, ... }:
{
  options.aoide.yazi.enable = lib.mkEnableOption "the yazi terminal file manager";

  config = lib.mkIf config.aoide.yazi.enable {
    home-manager.users.${config.aoide.user} = _: {
      programs.yazi = {
        enable = true;
        enableBashIntegration = true;
        shellWrapperName = "y";

        settings = {
          manager = {
            ratio = [
              0
              1
              1
            ];
            sort_by = "mtime";
            sort_sensitive = false;
            sort_reverse = true;
            linemode = "size";
            show_hidden = false;
          };

          opener = {
            edit = [
              {
                run = ''nvim "$@"'';
                block = true;
                desc = "Edit in nvim";
              }
            ];
            open = [
              {
                run = ''xdg-open "$@"'';
                orphan = true;
                desc = "Open (xdg-open)";
              }
            ];
          };

          open = {
            prepend_rules = [
              {
                mime = "text/*";
                use = [
                  "edit"
                  "open"
                ];
              }
              {
                mime = "image/*";
                use = "open";
              }
              {
                mime = "video/*";
                use = "open";
              }
              {
                mime = "audio/*";
                use = "open";
              }
              # Archives by mime rather than a brace-expansion name glob —
              # yazi's glob matcher is not confirmed to support `{a,b}`
              # alternation, and a silently-inert rule is exactly the bug
              # this dendrite exists to fix.
              {
                mime = "application/zip";
                use = "open";
              }
              {
                mime = "application/gzip";
                use = "open";
              }
              {
                mime = "application/x-tar";
                use = "open";
              }
              {
                mime = "application/x-bzip2";
                use = "open";
              }
              {
                mime = "application/x-xz";
                use = "open";
              }
              {
                mime = "application/x-7z-compressed";
                use = "open";
              }
              {
                mime = "application/vnd.rar";
                use = "open";
              }
            ];
            append_rules = [
              {
                url = "*";
                use = "open";
              }
            ];
          };
        };

        theme = {
          mgr = {
            preview_hovered = {
              underline = false;
            };
            folder_offset = [
              1
              0
              1
              0
            ];
            preview_offset = [
              1
              1
              1
              1
            ];
          };

          status.separator_style = {
            fg = "red";
            bg = "red";
          };
        };
      };

      home.shellAliases = {
        sy = "sudo yazi";
      };
    };
  };
}
