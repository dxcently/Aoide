#!/bin/sh
# fence.sh SOCK GATE_IP GATE_PORT -- COMMAND [ARG...]
#
# Runs COMMAND in a fresh user and network namespace (no privilege: `unshare -rn`).
# The namespace has one way out: a TCP bridge listening on GATE_IP:GATE_PORT
# inside it, carried over the unix socket SOCK to a host-side bridge that the
# caller raised. Nothing else routes, so COMMAND can reach the gate on that one
# port and nothing else. The bridge lives exactly as long as COMMAND does.
set -eu
export FENCE_SOCK=$1 GATE_IP=$2 GATE_PORT=$3
shift 4
exec unshare -rn sh -c '
  set -u
  ip link set lo up || exit 97
  if [ "$GATE_IP" != 127.0.0.1 ]; then ip addr add "$GATE_IP/32" dev lo || exit 97; fi
  socat TCP-LISTEN:"$GATE_PORT",bind="$GATE_IP",reuseaddr,fork UNIX-CONNECT:"$FENCE_SOCK" &
  bridge=$!
  porthex=$(printf "%04X" "$GATE_PORT")
  i=0
  until grep -q ":$porthex 00000000:0000 0A" /proc/net/tcp || [ "$i" -ge 200 ]; do i=$((i+1)); sleep 0.02; done
  grep -q ":$porthex 00000000:0000 0A" /proc/net/tcp || exit 98
  "$@"
  rc=$?
  kill "$bridge" 2>/dev/null || true
  exit "$rc"
' fence "$@"
