# lib/songbook-merge.nix — several songbook directories as ONE songbook.
#
#   aoide.lib.songbookMerge { inherit lib pkgs; books = [ aoide.songbookRoot ./songs ]; }
#
# Returns `lib/songbook.nix`'s record over the merged directory, plus `dir`: the
# merged directory itself, as a store path string. Both come from this one call,
# so the directory a host performs from, the songs discovery names and the
# registry they resolve through are the same value and cannot disagree. A
# consumer hands `dir` to the song hook (`songbook = …`) and to the lyra lane's
# per-host songbook, and reads `songNames` / `songs` / `rices` off the same
# record.
#
# Merge happens BEFORE borrow resolution: every song is copied into one
# directory first, so a song may borrow a slot from a song that came from
# another book (a consumer's song borrowing Aoide's sonata across books is the
# case this exists for: tests/consumer's `encore`). A song name that two books define is a refusal, not a choice: the throw
# names the song and every book that defines it, the same posture habit's
# `mergeRegistries` takes for a name two registries share.
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
  # copied from that book. Each entry is a directory in the songbook layout.
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

  copies = lib.concatMap (
    b:
    map (name: {
      inherit name;
      inherit (b) book;
    }) b.names
  ) perBook;

  merged = pkgs.runCommand "aoide-songbook-merged" { } (
    ''
      mkdir -p $out
    ''
    + lib.concatMapStrings (c: ''
      cp -r --no-preserve=mode ${c.book + "/${c.name}"} $out/${c.name}
    '') copies
  );
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
    let
      dir = "${merged}";
    in
    (songbookOf dir) // { inherit dir; }
  )
