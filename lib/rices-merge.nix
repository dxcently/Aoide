# lib/rices-merge.nix — several rices directories as ONE rices.
#
#   aoide.lib.rices.merge { inherit lib pkgs; books = [ aoide.ricesRoot ./rices ]; }
#
# Returns `{ registry, dir }` and the `lib/rices.nix` record over the merge:
#
#   registry  rice name -> that rice's directory INSIDE the book that defines it
#             (a pure readDir over the input directories, no derivation built).
#   dir       the merged directory, as a store path string: one copy of every
#             registry entry, built from the same registry in one call.
#
# The record's discovery is the registry, so the hook, borrow resolution and
# `builtIn` never read `dir` (no import-from-derivation); only the paint lane's
# rices package consumes `dir`'s contents. A consumer hands `dir` to the rice
# hook (`rices = …`) and to the strillix lane's per-host rices, and reads
# `discoveredNames` / `riceDirs` / `rices` off the same record.
#
# Merge happens BEFORE borrow resolution: every rice is visible in one registry
# first, so a rice may borrow a slot from a rice that came from another book (a
# consumer's rice borrowing Aoide's sonata across books is the case this exists
# for: tests/consumer's `encore`). A rice name that two books define is a
# refusal, not a choice: the throw names the rice and every book that defines
# it, the same posture habit's `mergeRegistries` takes for a name two registries
# share.
#
# Discovery is the one `lib/rices.nix` uses: an immediate child directory,
# `_`-prefixed entries shelved. A book's loose files (`learnings.md`, a
# `.gitkeep`) are not rices and are not copied.
{
  lib,
  # The store copy of the merged directory is a derivation; the caller's own
  # `pkgs` builds it (the same nixpkgs the host evaluates against).
  pkgs,
  # Ordered: a name defined by no book is absent, one defined by exactly one is
  # taken from that book. Each entry is a directory in the rices layout.
  books,
}:
let
  ricesOf =
    book:
    import ./rices.nix {
      inherit lib;
      ricesRoot = book;
    };

  perBook = map (book: {
    inherit book;
    names = (ricesOf book).discoveredNames;
  }) books;

  allNames = lib.unique (lib.concatMap (b: b.names) perBook);
  booksDefining = name: map (b: b.book) (lib.filter (b: builtins.elem name b.names) perBook);
  clashing = lib.filter (name: builtins.length (booksDefining name) > 1) allNames;

  registry = lib.listToAttrs (
    lib.concatMap (
      b:
      map (name: {
        inherit name;
        value = b.book + "/${name}";
      }) b.names
    ) perBook
  );

  merged = pkgs.runCommand "aoide-rices-merged" { } (
    ''
      mkdir -p $out
    ''
    + lib.concatStringsSep "" (
      lib.mapAttrsToList (name: path: ''
        cp -r --no-preserve=mode ${path} $out/${name}
      '') registry
    )
  );
  dir = "${merged}";
in
lib.throwIf (clashing != [ ])
  (
    "rices merge: rice(s) ${lib.concatStringsSep ", " clashing} defined by more than one book — "
    + lib.concatStringsSep "; " (
      map (
        name: "'${name}' is in ${lib.concatStringsSep " and " (map toString (booksDefining name))}"
      ) clashing
    )
    + "; rename one, or drop it from a book"
  )
  (
    {
      inherit registry dir;
    }
    // (import ./rices.nix {
      inherit lib;
      ricesRoot = dir;
      discovered = registry;
    })
  )
