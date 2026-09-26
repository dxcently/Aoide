# OpenAI tools share one host toggle.

let
  body =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    {
      options.aoide.openai.enable = lib.mkEnableOption "OpenAI Codex and ChatGPT Desktop";
      config = lib.mkIf config.aoide.openai.enable {
        nixpkgs.config.allowUnfree = true;
        environment.systemPackages = [
          pkgs.codex
          pkgs.chatgpt-linux
        ];
      };
    };
in
{
  inherit body;

  nixos =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.openai.enable = lib.mkDefault true;
    };
}
