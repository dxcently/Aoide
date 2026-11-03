#!/usr/bin/env perl
# tests/boundary/scan.pl — content of the `boundary` flake check (lib/checks.nix
# Check 14: lib/ wires it, this directory holds it).
#
# Aoide (core: the aoide/aoided binaries and aoide-desktop) and Strillix (the
# OS: the modules, the paint, the strillix binary) share one repo and name each
# other in many places. This script counts those places as ROWS and holds them
# against crossings.golden, so a crossing the golden lacks fails and a removed one
# is locked in:
#
#   perl tests/boundary/scan.pl check   <repo-root> <golden>
#   perl tests/boundary/scan.pl tighten <repo-root> <golden>
#
# SIDES. A file's side is decided by its path and nothing else:
#
#   Aoide side
#     - pkgs/aoide/**, except pkgs/aoide/crates/strillix/,
#       pkgs/aoide/crates/strillix-rice/ and pkgs/aoide/crates/screen/
#     - pkgs/aoide-desktop/**
#     - these modules/dendrites/ files: aoided.nix, aoide-config.nix,
#       aoide-nix.nix, aoide-packages.nix, aoide-secrets.nix, melete.nix,
#       melete-adapter.nix, mneme.nix, tailscale.nix
#   Strillix side
#     - pkgs/aoide/crates/strillix/, pkgs/aoide/crates/strillix-rice/,
#       pkgs/aoide/crates/screen/
#     - pkgs/strillix-shell/**, pkgs/strillix-rices/**
#     - everything else under modules/, including modules/default.nix and
#       modules/nucleus/
#     - lib/strillix.nix, lib/rice.nix, lib/rices.nix, lib/rices-merge.nix,
#       lib/theme.nix, lib/wallpaper-seed.sh
#     - rices/**
#   Neutral, not scanned: everything else. That includes flake.nix, the other
#   lib/ files, tests/, docs/, hosts/, users/, templates/, evals/, wallpapers/,
#   the other pkgs/ and the top-level files.
#
#   On both sides, every *.md file and every binary file (perl's -B test) is
#   skipped, and so is a symlink. The files are the ones a flake build sees:
#   the tracked ones. A checkout asks git for them, so build output and stray
#   files never count; the build sandbox has no .git and its source holds only
#   tracked files, so there the tree is walked.
#
# ROWS. Sorted, one per line, as the golden holds them:
#
#   name <side> <path> <count>
#     <count> lines of <path> name the other product; <side> is the file's own.
#     An Aoide-side file names Strillix on a line matching /strillix|\bstrx\b/i,
#     a Strillix-side file names Aoide on a line matching /aoide/i. A full-line
#     comment does not count. What a comment is depends on the extension (in
#     Rust `#[...]` is an attribute, not a comment):
#       .rs .qml .js                   first non-space characters `//`, `/*`, `*`
#       .nix .toml .sh .yaml .yml .py  first non-space character `#`
#       any other extension            no comment rule
#     Only a count above zero is a row.
#
#   dep <from-crate> <to-crate> <section>
#     a Cargo dependency edge from a crate on one side to a crate whose
#     directory is on the other. Every Cargo.toml on either side is read:
#     [dependencies], [dev-dependencies], [build-dependencies] and their
#     target-specific forms. A dependency's directory is its `path` (relative
#     to the manifest) or, for `workspace = true`, the path its
#     [workspace.dependencies] entry gives (relative to the workspace root the
#     crate inherits from). Crate names are the manifests' [package] names.
#     Regex-based like lib/checks.nix Check 7: a dependency line the regexes
#     cannot read fails the scan naming the line, never gets walked past.
#
# CHECK computes the rows and compares them with the golden. Equal rows pass
# with a one-line summary. Otherwise it fails, in two ways that can both occur:
#   (a) an unlisted crossing: a row the golden lacks, or one whose count is
#       higher. Each such row is printed exactly as the golden would hold it.
#   (b) a stale golden: a golden row whose crossing is gone, or whose count is
#       lower. The complete tightened golden is printed, ready to paste, and
#       only when there is no (a).
# TIGHTEN rewrites the golden from the tree. While any (a) exists it refuses —
# exits non-zero and writes nothing — so the golden can only shrink. Given no
# golden file at all it writes the first one, since there is nothing to widen.
use strict;
use warnings;
use File::Find ();

my ($mode, $root, $golden_file) = @ARGV;
die "usage: scan.pl check|tighten <repo-root> <golden>\n"
  unless @ARGV == 3 && $mode =~ /^(?:check|tighten)$/;
$root =~ s{(?<=.)/+$}{};

my $GOLDEN_AT = 'tests/boundary/crossings.golden';
my $TIGHTEN   = "perl tests/boundary/scan.pl tighten . $GOLDEN_AT";
my $GOLDEN_HEADER = <<'EOT';
# crossings.golden -- every place where Aoide's code and Strillix's code name
# each other, as tests/boundary/scan.pl counts them. One row per line, sorted:
#   name <side> <path> <count>             lines of <path> naming the other side
#   dep <from-crate> <to-crate> <section>  a Cargo edge from one side to the other
# Only `scan.pl tighten` rewrites this file, and it only removes or lowers rows.
# A deliberate crossing is a row added by hand, reasoned in its commit message.
EOT

sub slurp {
    my ($path) = @_;
    open my $fh, '<:raw', $path or die "boundary: cannot read $path: $!\n";
    local $/;
    return scalar <$fh>;
}

# ── Sides ────────────────────────────────────────────────────────────────────
my %AOIDE_DENDRITE = map { ("modules/dendrites/$_" => 1) } qw(
  aoided.nix aoide-config.nix aoide-nix.nix aoide-packages.nix aoide-secrets.nix
  melete.nix melete-adapter.nix mneme.nix tailscale.nix
);
my %STRILLIX_LIB = map { ("lib/$_" => 1) } qw(
  strillix.nix rice.nix rices.nix rices-merge.nix theme.nix wallpaper-seed.sh
);

sub side_of {
    my ($p) = @_;
    return 'strillix' if $p =~ m{^pkgs/aoide/crates/(?:strillix|strillix-rice|screen)/};
    return 'aoide'    if $p =~ m{^pkgs/aoide(?:-desktop)?/};
    return 'strillix' if $p =~ m{^pkgs/strillix-(?:shell|rices)/};
    return 'aoide'    if $AOIDE_DENDRITE{$p};
    return 'strillix' if $p =~ m{^(?:modules|rices)/} || $STRILLIX_LIB{$p};
    return undef;
}

# ── Files ────────────────────────────────────────────────────────────────────
sub repo_files {
    if (-e "$root/.git" && open(my $ls, '-|', 'git', '-C', $root, 'ls-files', '-z')) {
        local $/ = "\0";
        chomp(my @tracked = <$ls>);
        return @tracked if close $ls;
    }
    my @walked;
    File::Find::find({
        no_chdir => 1,
        wanted   => sub {
            if (-d $_) { $File::Find::prune = 1 if m{(?:^|/)\.git$}; return }
            (my $rel = $File::Find::name) =~ s{^\Q$root\E/}{};
            push @walked, $rel;
        },
    }, $root);
    return @walked;
}

my %side;    # path -> side, for the scanned files only
for my $p (repo_files()) {
    my $s = side_of($p) or next;
    next if -l "$root/$p" || !-f "$root/$p";
    $side{$p} = $s;
}

# ── name rows ────────────────────────────────────────────────────────────────
my %COMMENT = (
    (map { $_ => qr{^\s*(?://|/\*|\*)} } qw(rs qml js)),
    (map { $_ => qr{^\s*#} } qw(nix toml sh yaml yml py)),
);
my %NAMES = (aoide => qr/strillix|\bstrx\b/i, strillix => qr/aoide/i);

my (%now, %row_side);    # row key -> count, row key -> the side that does the naming

for my $p (sort keys %side) {
    next if $p =~ /\.md$/ || -B "$root/$p";
    my $s = $side{$p};
    my ($ext) = $p =~ /\.([^.\/]+)$/;
    my $comment = defined $ext ? $COMMENT{$ext} : undef;
    open my $fh, '<:raw', "$root/$p" or die "boundary: cannot read $p: $!\n";
    my $n = 0;
    while (my $line = <$fh>) {
        next if $comment && $line =~ $comment;
        $n++ if $line =~ $NAMES{$s};
    }
    close $fh;
    next unless $n;
    $now{"name $s $p"}      = $n;
    $row_side{"name $s $p"} = $s;
}

# ── dep rows ─────────────────────────────────────────────────────────────────
sub manifest_file { my ($dir) = @_; return $dir eq '.' ? 'Cargo.toml' : "$dir/Cargo.toml" }

sub parent_dir { my ($d) = @_; $d =~ s{/?[^/]+$}{}; return length $d ? $d : '.' }

# REL resolved against the repo-relative directory BASE; repo-relative result.
sub resolve_dir {
    my ($base, $rel, $where) = @_;
    my @out = $base eq '.' ? () : split m{/}, $base;
    for my $seg (split m{/}, $rel) {
        next if $seg eq '' || $seg eq '.';
        if ($seg eq '..') {
            @out or die "boundary: $where: the path `$rel` leaves the repository\n";
            pop @out;
        }
        else { push @out, $seg }
    }
    return @out ? join('/', @out) : '.';
}

sub unreadable {
    my ($file, $line) = @_;
    die "boundary: $file: a dependency line this scan cannot read; write it as a "
      . "single-line { path = \"...\" } or { workspace = true }, or teach "
      . "tests/boundary/scan.pl the form:\n$line\n";
}

sub unquote { my ($s) = @_; $s =~ s/^(["'])(.*)\1$/$2/; return $s }

# The manifest's lines with comments dropped and blanks gone, a value that spans
# lines joined onto one: a header or a `key = value` per element.
sub logical_lines {
    my ($text, $file) = @_;
    my (@out, $acc);
    for my $raw (split /\n/, $text) {
        (my $line = $raw) =~ s/^((?:[^"'#]|"(?:[^"\\]|\\.)*"|'[^']*')*)#.*$/$1/;
        next if !defined $acc && $line !~ /\S/;
        die "boundary: $file: a multi-line string, which this scan does not read\n"
          if $line =~ /"""|'''/;
        $acc = defined $acc ? "$acc $line" : $line;
        (my $bare = $acc) =~ s/"(?:[^"\\]|\\.)*"|'[^']*'//g;
        my $open  = () = $bare =~ /[\[{]/g;
        my $close = () = $bare =~ /[\]}]/g;
        next if $open != $close;
        $acc =~ s/^\s+|\s+$//g;
        push @out, $acc;
        undef $acc;
    }
    die "boundary: $file: a value that never closes: $acc\n" if defined $acc;
    return @out;
}

# One `key = value` of a dependency; the two keys that decide an edge are kept.
sub note_key {
    my ($props, $key, $value, $file, $line) = @_;
    if ($key eq 'path') {
        $value =~ /^(?:"([^"]*)"|'([^']*)')$/ or unreadable($file, $line);
        $props->{path} = $1 // $2;
    }
    elsif ($key eq 'workspace') {
        $value eq 'true' or unreadable($file, $line);
        $props->{workspace} = 1;
    }
}

# The whole value of a dependency written as `name = value`.
sub note_value {
    my ($props, $value, $file, $line) = @_;
    return if $value =~ /^["']/;    # a registry version
    $value =~ /^\{(.*)\}$/s or unreadable($file, $line);
    my $table = $1;
    for my $key (qw(path workspace)) {
        next unless $table =~ /(?<![\w-])$key\s*=\s*("[^"]*"|'[^']*'|[^\s,]+)/;
        note_key($props, $key, $1, $file, $line);
    }
}

my %MANIFEST;    # directory -> what the edges need of its Cargo.toml

sub manifest { my ($dir) = @_; return $MANIFEST{$dir} //= parse_manifest($dir) }

sub parse_manifest {
    my ($dir) = @_;
    my $file = manifest_file($dir);
    my $m    = { dir => $dir, name => undef, workspace => 0, deps => [], inherited => {} };
    my $kind = qr/(?:dev-|build-)?dependencies/;
    my $cfg  = qr/target\.(?:"(?:[^"\\]|\\.)*"|'[^']*'|[^.\s"']+)/;
    my ($ctx, %pending);
    my $flush = sub {
        for my $name (sort keys %pending) {
            if ($ctx->{into} eq 'inherited') { $m->{inherited}{$name} = $pending{$name} }
            else { push @{ $m->{deps} }, { section => $ctx->{section}, name => $name, props => $pending{$name} } }
        }
        %pending = ();
    };
    for my $line (logical_lines(slurp("$root/$file"), $file)) {
        if ($line =~ /^\[(.*)\]$/) {
            $flush->() if $ctx && $ctx->{into};
            (my $h = $1) =~ s/^\[|\]$//g;    # [[an.array.of.tables]]
            $h =~ s/^\s+|\s+$//g;
            undef $ctx;
            if    ($h eq 'package')                  { $ctx = { package => 1 } }
            elsif ($h eq 'workspace')                { $m->{workspace} = 1 }
            elsif ($h eq 'workspace.dependencies')   { $ctx = { into => 'inherited' } }
            elsif ($h =~ /^workspace\.dependencies\.(.+)$/) {
                $ctx = { into => 'inherited', table => unquote($1) };
            }
            elsif ($h =~ /^((?:$cfg\.)?$kind)$/)     { $ctx = { into => 'deps', section => $1 } }
            elsif ($h =~ /^((?:$cfg\.)?$kind)\.(.+)$/) {
                $ctx = { into => 'deps', section => $1, table => unquote($2) };
            }
            elsif ($h =~ /dependenc/) { die "boundary: $file: a dependency table this scan does not read: [$h]\n" }
            $pending{ $ctx->{table} } = {} if $ctx && defined $ctx->{table};
            next;
        }
        next unless $ctx;
        if ($ctx->{package}) {
            $m->{name} = $1 if $line =~ /^name\s*=\s*"([^"]*)"$/;
        }
        elsif (defined $ctx->{table}) {
            $line =~ /^([\w-]+)\s*=\s*(.+)$/ or unreadable($file, $line);
            note_key($pending{ $ctx->{table} }, $1, $2, $file, $line);
        }
        else {
            $line =~ /^("[^"]+"|'[^']+'|[\w-]+)(?:\.([\w-]+))?\s*=\s*(.+)$/ or unreadable($file, $line);
            my ($raw, $key, $value) = ($1, $2, $3);
            my $props = $pending{ unquote($raw) } //= {};
            if (defined $key) { note_key($props, $key, $value, $file, $line) }
            else              { note_value($props, $value, $file, $line) }
        }
    }
    $flush->() if $ctx && $ctx->{into};
    return $m;
}

# The nearest manifest at or above DIR that holds a [workspace].
sub workspace_root {
    my ($dir) = @_;
    for (my $d = $dir; ; $d = parent_dir($d)) {
        my $file = manifest_file($d);
        if (-f "$root/$file") {
            my $m = manifest($d);
            return $m if $m->{workspace};
        }
        return undef if $d eq '.';
    }
}

for my $p (sort keys %side) {
    $p =~ m{^(?:(.*)/)?Cargo\.toml$} or next;
    my $dir  = $1 // '.';
    my $from = manifest($dir);
    defined $from->{name} or next;    # a virtual workspace root is no crate
    for my $dep (@{ $from->{deps} }) {
        my ($props, $base) = ($dep->{props}, $dir);
        if ($props->{workspace}) {
            my $ws = workspace_root($dir)
              or die "boundary: $p: `$dep->{name}` inherits from a workspace, and none holds this crate\n";
            $props = $ws->{inherited}{ $dep->{name} }
              or die "boundary: $p: `$dep->{name}` inherits from a workspace that does not list it\n";
            $base = $ws->{dir};
        }
        defined $props->{path} or next;    # a registry crate has no directory here
        my $to_dir  = resolve_dir($base, $props->{path}, $p);
        my $to_side = side_of(manifest_file($to_dir)) or next;    # neutral: not scanned
        next if $to_side eq $side{$p};
        -f "$root/" . manifest_file($to_dir)
          or die "boundary: $p: `$dep->{name}` points at $to_dir, which holds no Cargo.toml\n";
        my $to = manifest($to_dir)->{name}
          // die "boundary: $p: `$dep->{name}` points at $to_dir, whose Cargo.toml names no package\n";
        my $row = "dep $from->{name} $to $dep->{section}";
        $now{$row}      = 1;
        $row_side{$row} = $side{$p};
    }
}

# ── Golden ───────────────────────────────────────────────────────────────────
sub row { my ($key, $count) = @_; return $key =~ /^name / ? "$key $count" : $key }

sub golden_text { return $GOLDEN_HEADER . join('', map { row($_, $now{$_}) . "\n" } sort keys %now) }

sub read_golden {
    my %gold;
    open my $fh, '<', $golden_file or die "boundary: cannot read $golden_file: $!\n";
    while (my $line = <$fh>) {
        chomp $line;
        next if $line =~ /^#/ || $line !~ /\S/;
        my ($key, $count);
        if    ($line =~ /^(name (?:aoide|strillix) .+) (\d+)$/) { ($key, $count) = ($1, $2) }
        elsif ($line =~ /^(dep \S+ \S+ .+)$/)                  { ($key, $count) = ($1, 1) }
        else { die "boundary: $golden_file:$.: not a row: $line\n" }
        die "boundary: $golden_file:$.: a second row for: $key\n" if exists $gold{$key};
        $gold{$key} = $count;
    }
    close $fh;
    return %gold;
}

my $have_golden = -e $golden_file;
die "boundary: no golden at $golden_file; `$TIGHTEN` writes the first one\n"
  if !$have_golden && $mode eq 'check';
my %gold = $have_golden ? read_golden() : ();

my @new   = grep { !exists $gold{$_} || $now{$_} > $gold{$_} } sort keys %now;
my @stale = grep { !exists $now{$_} || $now{$_} < $gold{$_} } sort keys %gold;

if (@new && ($mode eq 'check' || $have_golden)) {
    print STDERR "boundary: Aoide and Strillix name each other in ", scalar(@new),
      " place(s) the golden does not allow:\n";
    print STDERR row($_, $now{$_}), "\n" for @new;
    print STDERR "If this crossing is deliberate, add the row to $GOLDEN_AT by hand and give "
      . "the reason in the commit message; tighten never adds rows.\n";
    print STDERR "The golden also holds ", scalar(@stale), " row(s) whose crossing is gone or "
      . "fewer; settle the rows above, then tighten.\n" if @stale;
    print STDERR "tighten refused and wrote nothing.\n" if $mode eq 'tighten';
    exit 1;
}

if ($mode eq 'tighten') {
    my $text = golden_text();
    if ($have_golden && $text eq slurp($golden_file)) {
        print "boundary: $golden_file is already tight (", scalar(keys %now), " rows)\n";
        exit 0;
    }
    open my $out, '>', $golden_file or die "boundary: cannot write $golden_file: $!\n";
    print $out $text;
    close $out or die "boundary: cannot write $golden_file: $!\n";
    printf "boundary: wrote %s (%d rows%s)\n", $golden_file, scalar(keys %now),
      $have_golden ? sprintf(', %d removed or lowered', scalar @stale) : '';
    exit 0;
}

if (@stale) {
    print STDERR "boundary: the golden is stale; these crossings are gone or fewer:\n";
    for my $key (@stale) {
        print STDERR exists $now{$key}
          ? "fewer: " . row($key, $gold{$key}) . " -> $now{$key}\n"
          : "gone:  " . row($key, $gold{$key}) . "\n";
    }
    print STDERR "The tightened golden, ready to paste as $GOLDEN_AT:\n",
      "----8<----\n", golden_text(), "----8<----\n",
      "Or run, from the repo root: $TIGHTEN\n";
    exit 1;
}

my %tally;
$tally{ (/^(\w+)/)[0] . " $row_side{$_}" }++ for keys %now;
printf "aoide check boundary: ok (%s)\n",
  join ', ', map { "$_ " . ($tally{$_} // 0) } ('name aoide', 'name strillix', 'dep aoide', 'dep strillix');
