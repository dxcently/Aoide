# OpenAI tools share one host toggle.

{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.openai.enable = lib.mkEnableOption "OpenAI Codex and ChatGPT Desktop";
  config = lib.mkMerge [
    { aoide.openai.enable = lib.mkDefault true; }
    (lib.mkIf config.aoide.openai.enable {
      nixpkgs.config.allowUnfree = true;
      environment.systemPackages = [
        pkgs.codex
        pkgs.chatgpt-linux
      ];
    })
  ];
}
