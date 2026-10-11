# OpenAI tools share one host toggle.

{
  lib,
  pkgs,
  ...
}:
{
  config = {
    nixpkgs.config.allowUnfree = true;
    environment.systemPackages = [
      pkgs.codex
      pkgs.chatgpt-linux
    ];
  };
}
