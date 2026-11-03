# A provider set no host selects. Reading it throws, so the thin host evaluating at all is the
# proof that the renderer never forced it.
throw "thin-flake: an unselected provider set was read"
