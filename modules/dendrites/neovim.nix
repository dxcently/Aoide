# modules/dendrites/neovim.nix — Neovim via nvf for the aoide user.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.neovim.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does:
#   - The full dxflake nvf (Neovim-Flake) stack, ported from dxflake
#     modules/dendrites/neovim.nix: rose-pine theme, LSP + treesitter,
#     telescope, which-key, neo-tree, dashboards, lualine, bufferline,
#     nvim-cmp/luasnip, the complete keymap set (clipboard leaders,
#     insert-mode hjkl, neo-tree/cheatsheet toggles).
#   - nvf rides in through the `nvf` flake input; this dendrite imports
#     `inputs.nvf.homeManagerModules.default` into the aoide user's HM config.
#     `inputs` arrives via specialArgs (mkHost and the VM test both pass it),
#     so the HM submodule reaches nvf by closure — no extraSpecialArgs needed
#     here, though mkHost threads them for future dendrites.
#
# LANGUAGES — Aoide keeps enabled only what Aoide work actually uses:
#   nix (this repo) · rust (aoide CLI/aoided) · ts/js (notes package) ·
#   bash · markdown. The REST of dxflake's language set (python, clang, html,
#   lua, typst, css, tailwind preset) is preserved below COMMENTED OUT with
#   each section intact — re-enabling a language is uncommenting one block.
#
# Adapted vs dxflake (fresh nvf pin; renames noted inline):
#   - dxflake also ran a parallel bare `programs.neovim` (vimAlias = true)
#     next to nvf; folded here into nvf's own vimAlias/viAlias so only ONE
#     nvim (the nvf wrapper) lands in the profile.
#   - languages.nix format: alejandra → nixfmt, matching Aoide's flake
#     formatter so format-on-save agrees with `nix fmt`.
{
  config,
  lib,
  inputs,
  ...
}:
{
  options.aoide.neovim.enable = lib.mkEnableOption "Neovim via nvf (dxflake stack: rose-pine, LSP/treesitter, telescope, neo-tree)";

  config = lib.mkIf config.aoide.neovim.enable {
    home-manager.users.${config.aoide.user} = {
      imports = [ inputs.nvf.homeManagerModules.default ];

      # nvf provides the only `nvim` in the profile (header), so the dendrite
      # that installs the editor also declares it the default — EDITOR/VISUAL
      # for login shells, TTY, and SSH on this single-user box. Guarded by the
      # same aoide.neovim.enable, so the editor default never outlives nvim.
      home.sessionVariables = {
        EDITOR = "nvim";
        VISUAL = "nvim";
      };

      programs.nvf = {
        enable = true;

        settings.vim = {
          # dxflake split aliases across nvf + a bare programs.neovim; both
          # live on nvf's wrapper here (see header).
          vimAlias = true;
          viAlias = true;

          # mkForce: the Stylix facet's auto-imported HM nvf target pushes a
          # base16 theme at nvf; dxflake pinned rose-pine over its stylix the
          # same way. Forced so the dendrite's pick wins deterministically.
          theme = lib.mkForce {
            enable = true;
            name = "rose-pine";
            style = "main";
          };

          keymaps = [
            {
              key = "<leader>y";
              mode = [
                "v"
                "n"
              ];
              action = ''"+y'';
              desc = "Yank into system clipboard";
            }
            {
              key = "<leader>d";
              mode = [
                "v"
                "n"
              ];
              action = ''"+d'';
              desc = "Cut into system clipboard";
            }
            {
              key = "<leader>Y";
              mode = [
                "v"
                "n"
              ];
              action = ''"+yg_'';
              desc = "Yank to the right of cursor into system clipboard";
            }
            {
              key = "<leader>p";
              mode = [
                "v"
                "n"
              ];
              action = ''"+p'';
              desc = "Paste from system clipboard";
            }
            {
              key = "<leader>P";
              mode = [
                "v"
                "n"
              ];
              action = ''"+P'';
              desc = "Paste to the left of cursor from system clipboard";
            }
            {
              key = "<C-h>";
              mode = [ "i" ];
              action = "<Left>";
              desc = "Move left in insert mode";
            }
            {
              key = "<C-j>";
              mode = [ "i" ];
              action = "<Down>";
              desc = "Move down in insert mode";
            }
            {
              key = "<C-k>";
              mode = [ "i" ];
              action = "<Up>";
              desc = "Move up in insert mode";
            }
            {
              key = "<C-l>";
              mode = [ "i" ];
              action = "<Right>";
              desc = "Move right in insert mode";
            }
            {
              key = "<leader>nt";
              mode = [ "n" ];
              action = "<cmd>Neotree toggle<cr>";
              desc = "File browser toggle";
            }
            {
              key = "<leader>nh";
              mode = [ "n" ];
              action = ":nohl<CR>";
              desc = "Clear search highlights";
            }
            {
              key = "<leader>?";
              mode = [ "n" ];
              action = "<cmd>Cheatsheet<cr>";
              desc = "Opens cheatsheet.nvim";
            }
          ];

          options = {
            autoindent = true;
            wrap = false;
            tabstop = 4;
            shiftwidth = 4;
            termguicolors = true;
          };

          syntaxHighlighting = true;

          telescope.enable = true;
          spellcheck = {
            enable = false;
          };

          lsp = {
            enable = true;
            formatOnSave = true;
            lspkind.enable = true;
            trouble.enable = true;
            lspSignature.enable = true;
            nvim-docs-view.enable = false;
            inlayHints.enable = false;
            # Tailwind is web-frontend tooling Aoide doesn't use — shelved with
            # the css block below.
            # presets.tailwindcss-language-server.enable = true;
          };

          languages = {
            enableFormat = true;
            enableTreesitter = true;
            enableExtraDiagnostics = true;

            # ── Enabled: the Aoide working set ────────────────────────────
            bash.enable = true;
            rust.enable = true; # aoide CLI / aoided
            typescript.enable = true; # notes package (ts/js + node)
            nix = {
              enable = true; # this repo
              format = {
                enable = true;
                # dxflake used alejandra; Aoide's flake formatter is nixfmt —
                # keep the two in agreement.
                type = [ "nixfmt" ];
              };
            };
            markdown = {
              enable = true;
              extensions = {
                #markview-nvim.enable = true;
                render-markdown-nvim.enable = true;
              };
            };

            # ── Shelved: the rest of dxflake's language set ───────────────
            # Preserved intact; re-enable a language by uncommenting its block.
            # python.enable = true;
            # clang.enable = true;
            # html.enable = true;
            # lua.enable = true;
            # typst = {
            #   enable = true;
            #   extensions.typst-preview-nvim.enable = true;
            #   format = {
            #     enable = true;
            #     type = [ "typstyle" ];
            #   };
            # };
            # css = {
            #   enable = true;
            #   format.enable = true;
            #   lsp.enable = true;
            # };
          };

          visuals = {
            nvim-cursorline.enable = true;
            fidget-nvim.enable = true;

            highlight-undo.enable = true;
            indent-blankline.enable = true;
          };

          statusline = {
            lualine = {
              enable = true;
              # mkForce: same Stylix nvf-target collision as theme above (it
              # pins lualine to "base16"); dxflake ran lualine on "auto".
              theme = lib.mkForce "auto";
            };
          };

          autopairs.nvim-autopairs.enable = true;

          autocomplete.nvim-cmp.enable = true;
          snippets.luasnip.enable = true;

          tabline = {
            nvimBufferline.enable = true;
          };

          treesitter = {
            context.enable = false; # annoying
            indent.enable = false; # annoying indenter
          };

          binds = {
            whichKey.enable = true;
            cheatsheet.enable = true;
          };

          git = {
            enable = true;
            gitsigns.enable = true;
            gitsigns.codeActions.enable = false; # throws an annoying debug message
          };

          filetree.neo-tree = {
            enable = true;
          };

          dashboard = {
            dashboard-nvim.enable = true;
            alpha.enable = true;
          };

          notify = {
            nvim-notify.enable = true;
          };

          utility = {
            ccc.enable = true;
            icon-picker.enable = false;
            surround.enable = true;
            diffview-nvim.enable = true;
            nix-develop.enable = true;
            motion = {
              precognition.enable = true;
            };

            images = {
              image-nvim.enable = false;
            };
          };

          ui = {
            borders.enable = true;
            noice.enable = true;
            colorizer.enable = true;
            illuminate.enable = true;
            smartcolumn = {
              enable = true;
            };
            fastaction.enable = true;
          };

          session = {
            nvim-session-manager.enable = false;
          };

          comments = {
            comment-nvim.enable = true;
          };
        };
      };
    };
  };
}
