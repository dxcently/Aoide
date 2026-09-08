# OpenAI tools: Codex CLI and the community ChatGPT Linux desktop flake.
{
  config,
  inputs,
  lib,
  pkgs,
  ...
}:
let
  upstream = inputs.chatgpt-desktop-linux.packages.${pkgs.stdenv.hostPlatform.system}.chatgpt-desktop;
  dmg = pkgs.fetchurl {
    url = "https://persistent.oaistatic.com/codex-app-prod/ChatGPT.dmg";
    hash = "sha256-tv/tc9WBBHhi6F3ltNMiukMQBJSaUzhzbnBZGC3/YII=";
  };
  staleDmg = dmg.overrideAttrs (_: {
    outputHash = "sha256-TukDFPYFaGI+WE63hQuBc3d307761tMCi9+oco6sImU=";
  });
  # The pinned upstream flake hashes an older payload at a mutable URL.
  payload = upstream.src.overrideAttrs (old: {
    installPhase = lib.replaceStrings [ "${staleDmg}" ] [ "${dmg}" ] (
      builtins.appendContext (builtins.unsafeDiscardStringContext old.installPhase)
        (lib.filterAttrs (name: _: !(lib.hasSuffix "-ChatGPT.dmg.drv" name))
          (builtins.getContext old.installPhase))
    );
  });
in
{
  imports = [ inputs.chatgpt-desktop-linux.nixosModules.default ];

  options.aoide.openai.enable = lib.mkEnableOption "OpenAI Codex and ChatGPT Desktop";

  config = lib.mkIf config.aoide.openai.enable {
    environment.systemPackages = [ pkgs.codex ];
    programs.chatgptDesktopLinux.enable = true;
    programs.chatgptDesktopLinux.package = upstream.overrideAttrs (_: { src = payload; });
    programs.chatgptDesktopLinux.cliPackage = pkgs.codex;
  };
}
