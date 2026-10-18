# lib/songbook-merge.nix — several songbook directories as ONE songbook.
#
#   aoide.lib.songbookMerge { inherit lib pkgs; books = [ aoide.songbookRoot ./songs ]; }
#
# Returns `{ registry, dir }` and the `lib/songbook.nix` record over the merge:
#
#   registry  song name -> that song's directory INSIDE the book that defines it
#             (a pure readDir over the input directories, no derivation built).
#   dir       the merged directory, as a store path string: one copy of every
#             registry entry, built from the same registry in one call.
#
# The record's discovery is the registry, so the hook, borrow resolution and
# `builtIn` never read `dir` (no import-from-derivation); only the paint lane's
# songbook package consumes `dir`'s contents. A consumer hands `dir` to the song
# hook (`songbook = …`) and to the lyra lane's per-host songbook, and reads
# `songNames` / `songs` / `rices` off the same record.
#
# Merge happens BEFORE borrow resolution: every song is visible in one registry
# first, so a song may borrow a slot from a song that came from another book (a
# consumer's song borrowing Aoide's sonata across books is the case this exists
# for: tests/consumer's `encore`). A song name that two books define is a
# refusal, not a choice: the throw names the song and every book that defines
# it, the same posture habit's `mergeRegistries` takes for a name two registries
# share.
#
# Discovery is the one `lib/songbook.nix` uses: an immediate child directory,
# `_`-prefixed entries shelved. A book's loose files (`learnings.md`, a
# `.gitkeep`) are not songs and are not copied.
{
  lib,
  # The store copy of the merged directory is a derivation; the caller's own
  # `pkgs` builds it (the same nixpkgs the host evaluates against).
  pkgs,
  # Ordered: a name defined by no book is absent, one defined by exactly one is
  # taken from that book. Each entry is a directory in the songbook layout.
  books,
}:
let
  songbookOf =
    book:
    import ./songbook.nix {
      inherit lib;
      songbook = book;
    };

  perBook = map (book: {
    inherit book;
    names = (songbookOf book).songNames;
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

  merged = pkgs.runCommand "aoide-songbook-merged" { } (
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
    "songbook merge: song(s) ${lib.concatStringsSep ", " clashing} defined by more than one book — "
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
    // (import ./songbook.nix {
      inherit lib;
      songbook = dir;
      discovered = registry;
    })
  )
