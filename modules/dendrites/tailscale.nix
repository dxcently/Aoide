# modules/dendrites/tailscale.nix — the system Tailscale client.
#
# Dendrite shape (CONTRACTS.md §2): guarded on aoide.tailscale.enable, carries
# its own dependency. Runs the stock NixOS `services.tailscale` daemon (kernel TUN,
# system socket), so the host gets a `tailscale0` interface, the `tailscale`
# CLI, and MagicDNS names that resolve for every program — not only a proxied
# browser.
#
# What it deliberately does NOT do:
#   - No auth key. Enrollment is interactive (`tailscale up`, then the browser
#     link), so no secret lives in the store or the repo.
#   - No subnet routes accepted, no exit node, no Funnel, no Tailscale SSH.
#   - Never touches the mneme dendrite's dedicated userspace node
#     (`tailscaled-mneme`, own socket and statedir) or any other userspace
#     client a user runs by hand; the system daemon owns only
#     /run/tailscale/tailscaled.sock and /var/lib/tailscale.
#
# `aoide.user` is made the Tailscale operator so the CLI works without sudo.
# DNS is left to the host: this machine's /etc/resolv.conf is resolvconf-managed,
# which the daemon integrates with, so no resolved is introduced.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.aoide.tailscale;
in
{
  options.aoide.tailscale = {
    enable = lib.mkEnableOption "the system Tailscale client (tailscaled + CLI)";

    acceptRoutes = lib.mkEnableOption "accepting subnet routes advertised by other tailnet nodes";
  };

  config = lib.mkMerge [
    { aoide.tailscale.enable = lib.mkDefault true; }
    (lib.mkIf cfg.enable {
      services.tailscale = {
        enable = true;
        package = pkgs.tailscale;
        # "client" turns on the reverse-path/forwarding sysctls Tailscale
        # needs to accept routes; plain "none" is enough when it does not.
        useRoutingFeatures = if cfg.acceptRoutes then "client" else "none";
        extraSetFlags = [
          "--operator=${config.aoide.user}"
          "--accept-routes=${lib.boolToString cfg.acceptRoutes}"
        ];
      };

      # Only the daemon's UDP port is opened, so direct paths can form
      # instead of relaying through DERP. The tailnet interface is NOT
      # trusted: tailnet peers reach only what the host already exposes
      # (sshd, which is key-only), never every local port.
      services.tailscale.openFirewall = true;
    })
  ];
}
