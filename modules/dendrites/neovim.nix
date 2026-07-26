# modules/dendrites/neovim.nix — Neovim for the aoide user.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.neovim.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does:
#   - programs.neovim for the aoide user, aliased as `vi`/`vim` (the bash
#     dendrite also maps `v`/`nv` → nvim), with a small always-on base config
#     (clipboard yank/paste leaders, insert-mode hjkl motions, sane tab/indent
#     defaults) carried inline as Lua.
#
# ADAPTED vs dxflake — the big one:
#   dxflake's neovim.nix was an `nvf` (Neovim-Flake) configuration pulled from
#   `inputs.nvf.homeManagerModules.default`. Aoide's flake declares no `nvf`
#   input, and mkHost.nix passes no `extraSpecialArgs` into home-manager, so a
#   dendrite cannot reach `inputs.nvf` from inside the HM submodule. The entire
#   nvf tree (rose-pine theme, LSP/treesitter per-language stack, telescope,
#   which-key, neo-tree, dashboards, the full keymap set, …) is therefore
#   OMITTED here. To restore it: add the `nvf` flake input, thread it into HM
#   via `home-manager.extraSpecialArgs` in lib/mkHost.nix, then port the nvf
#   `settings.vim` block back. What remains below is a faithful, dependency-free
#   Neovim so the dendrite is useful today; the clipboard leaders and insert-mode
#   motions preserve the muscle-memory bits of the dxflake keymap.
{ config, lib, ... }:
{
  options.aoide.neovim.enable = lib.mkEnableOption "Neovim for the aoide user (plain base config — the dxflake nvf stack is omitted, see file comment)";

  config = lib.mkIf config.aoide.neovim.enable {
    home-manager.users.${config.aoide.user} =
      { ... }:
      {
        programs.neovim = {
          enable = true;
          viAlias = true;
          vimAlias = true;
          withRuby = false;
          withPython3 = false;

          initLua = ''
            -- sane base defaults (dxflake nvf options.*)
            vim.opt.autoindent = true
            vim.opt.wrap = false
            vim.opt.tabstop = 4
            vim.opt.shiftwidth = 4
            vim.opt.termguicolors = true

            -- system-clipboard leaders (dxflake keymaps, preserved)
            vim.keymap.set({ "v", "n" }, "<leader>y", '"+y', { desc = "Yank into system clipboard" })
            vim.keymap.set({ "v", "n" }, "<leader>d", '"+d', { desc = "Cut into system clipboard" })
            vim.keymap.set({ "v", "n" }, "<leader>Y", '"+yg_', { desc = "Yank to EOL into system clipboard" })
            vim.keymap.set({ "v", "n" }, "<leader>p", '"+p', { desc = "Paste from system clipboard" })
            vim.keymap.set({ "v", "n" }, "<leader>P", '"+P', { desc = "Paste-before from system clipboard" })

            -- insert-mode hjkl motions (dxflake keymaps, preserved)
            vim.keymap.set("i", "<C-h>", "<Left>", { desc = "Move left in insert mode" })
            vim.keymap.set("i", "<C-j>", "<Down>", { desc = "Move down in insert mode" })
            vim.keymap.set("i", "<C-k>", "<Up>", { desc = "Move up in insert mode" })
            vim.keymap.set("i", "<C-l>", "<Right>", { desc = "Move right in insert mode" })

            -- clear search highlight (dxflake <leader>nh)
            vim.keymap.set("n", "<leader>nh", ":nohl<CR>", { desc = "Clear search highlights" })
          '';
        };
      };
  };
}
