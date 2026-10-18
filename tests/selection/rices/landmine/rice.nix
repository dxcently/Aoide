# tests/selection/rices/landmine/rice.nix — a rice that must never be read
# unless it was SELECTED.
#
# The paired cases (`riceLandmineUnread`, `riceLandmineFires`) are the proof
# that a host imports only the rices it built in: a discovered-but-unselected
# rice's module is never imported, and the same file throws the moment it is.
throw "rices landmine: an unselected rice's rice.nix was imported"
