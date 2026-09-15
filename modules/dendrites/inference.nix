# modules/dendrites/inference.nix — local LLM inference tooling: Ollama and
# llama.cpp on PATH, CPU/RAM-backed.
#
# Dendrite shape v0 (CONTRACTS.md §2): guarded on aoide.inference.enable,
# carries its own dependencies, reads no other module.
#
# Plain packages, no GPU override: yomi-strix's unified LPDDR5X (~256GB/s)
# makes CPU inference genuinely usable, and it sidesteps picking a GPU
# backend outright — llama.cpp alone is a real fork here (vulkanSupport vs
# rocmSupport), while nixpkgs ships Ollama only as ollama / ollama-rocm /
# ollama-cuda, no Vulkan build at all. GPU acceleration is a follow-up
# override on this same file when wanted, not a redesign; ~/dxflake's
# inference.nix (ollama-rocm + llama-cpp rocmSupport, gfx1151 riding gfx11
# tensile libs via HSA_OVERRIDE_GFX_VERSION=11.0.0 + OLLAMA_IGPU_ENABLE=1,
# the latter logged as required — Ollama silently drops to CPU without it)
# is the proven precedent for that pass on this exact chip.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.inference.enable = lib.mkEnableOption "local LLM inference tooling (Ollama + llama.cpp)";

  config = lib.mkIf config.aoide.inference.enable {
    environment.systemPackages = [
      pkgs.ollama
      pkgs.llama-cpp
    ];
  };
}
