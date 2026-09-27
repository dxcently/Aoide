# tests/selection/songbook/landmine/rice.nix — a song that must never be read
# unless it was SELECTED.
#
# The paired cases (`songLandmineUnread`, `songLandmineFires`) are the proof
# that a host imports only the songs it built in: a discovered-but-unselected
# song's module is never imported, and the same file throws the moment it is.
throw "songbook landmine: an unselected song's rice.nix was imported"
