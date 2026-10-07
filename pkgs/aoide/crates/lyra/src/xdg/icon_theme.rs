//! Icon names resolved to files, for one nominal size at scale 1.
//!
//! [`Resolver::new`] reads the theme chain and lists each of its directories
//! once, so a [`Resolver`] is a snapshot of the icon dirs; [`Resolver::resolve`]
//! then answers a name or an absolute path with an existing `.png`, `.svg` or
//! `.xpm` file, or nothing. `icon-theme.cache` is never read.
//!
//! - **Theme.** `gtk-icon-theme-name` from `<config home>/gtk-3.0/settings.ini`
//!   when some base dir holds `<theme>/index.theme`, else `hicolor`. The chain
//!   is that theme, its `Inherits` depth-first (a repeat is skipped, so a
//!   cycle ends), then `hicolor`. A theme's index is the `index.theme` of the
//!   first base dir that has one. Base dirs are `$HOME/.icons` then each data
//!   dir's `icons/`.
//! - **Per theme.** Only scale-1 subdirs count. Subdirs go in `Directories`
//!   then `ScaledDirectories` order, then base dirs, then extensions `png`,
//!   `svg`, `xpm`; the subdir nearest the request that holds the name wins,
//!   and a tie keeps the first. A subdir's distance is how far the request
//!   lies outside its range: `Fixed` is `|Size - size|`, `Scalable` is outside
//!   `MinSize..=MaxSize`, `Threshold` is outside `Size ± Threshold`, and 0 is
//!   an exact match. A `Fixed` size below the request ranks after every
//!   `Fixed` size at or above it, so two `Fixed` subdirs of 32 and 128 give
//!   128 for a 48 px request. The first theme in the chain with any file for
//!   the name decides.
//! - **Unthemed.** The first `<name>.<ext>` in the base dirs themselves, each
//!   data dir's `pixmaps/`, then `/usr/share/pixmaps`.
//! - **Paths.** An absolute name resolves to itself when it is an existing
//!   regular file with one of the three extensions. The empty name, and a
//!   relative name holding `/`, resolve to nothing.
//!
//! Measured against Qt's own picks. Over 560 synthetic subdir sets (15
//! subdir shapes, in pairs and triples) a scratch Quickshell never picked a
//! file outside the nearest set this rule names; among tied subdirs it takes
//! the later one, or follows `icon-theme.cache` when a theme has one, and this
//! rule keeps the first. On osaka, for 48 px, 88 of 89 apps got Qt's file, a
//! byte-identical copy of it, or an icon Qt found nothing for; vesktop is the
//! exception: Qt took its `Scalable` 256 px, 16 px outside range, over the
//! `Threshold` 32 px, 14 px outside it.

use super::keyfile::KeyFile;
use super::Env;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const HICOLOR: &str = "hicolor";
const EXTENSIONS: [&str; 3] = ["png", "svg", "xpm"];

pub struct Resolver {
    theme: String,
    chain: Vec<Theme>,
    unthemed: Vec<PathBuf>,
}

struct Theme {
    name: String,
    dirs: Vec<Subdir>,
}

struct Subdir {
    kind: Kind,
    size: u32,
    min: u32,
    max: u32,
    threshold: u32,
    /// The `<base>/<theme>/<subdir>` directories that exist, in base-dir order.
    places: Vec<Place>,
}

/// One directory and the names in it, listed once: a lookup is a set probe, not a `stat` per extension per directory.
struct Place {
    dir: PathBuf,
    names: HashSet<OsString>,
}

impl Place {
    fn list(dir: PathBuf) -> Option<Place> {
        let names = std::fs::read_dir(&dir).ok()?.filter_map(Result::ok).map(|e| e.file_name()).collect();
        Some(Place { dir, names })
    }
}

enum Kind {
    Fixed,
    Scalable,
    Threshold,
}

impl Theme {
    fn find(&self, name: &str, size: u32) -> Option<PathBuf> {
        let files = EXTENSIONS.map(|ext| format!("{name}.{ext}"));
        let mut nearest: Option<((bool, u32), PathBuf)> = None;
        for dir in &self.dirs {
            let gap = dir.gap(size);
            for place in &dir.places {
                for file in files.iter().filter(|f| place.names.contains(OsStr::new(f))) {
                    let path = place.dir.join(file);
                    if !path.is_file() {
                        continue;
                    }
                    if gap == (false, 0) {
                        return Some(path);
                    }
                    if nearest.as_ref().is_none_or(|(g, _)| gap < *g) {
                        nearest = Some((gap, path));
                    }
                }
            }
        }
        nearest.map(|(_, path)| path)
    }
}

impl Subdir {
    /// How far this subdir is from `size`; smaller is nearer, `(false, 0)` is an exact match.
    /// A `Fixed` size below the request ranks after every `Fixed` size at or above it.
    fn gap(&self, size: u32) -> (bool, u32) {
        match self.kind {
            Kind::Fixed => (self.size < size, self.size.abs_diff(size)),
            Kind::Scalable => (false, outside(self.min, self.max, size)),
            Kind::Threshold => (false, outside(self.size.saturating_sub(self.threshold), self.size + self.threshold, size)),
        }
    }
}

fn outside(low: u32, high: u32, size: u32) -> u32 {
    low.saturating_sub(size).max(size.saturating_sub(high))
}

impl Resolver {
    pub fn new(env: &Env) -> Resolver {
        let bases: Vec<PathBuf> =
            std::iter::once(env.home.join(".icons")).chain(env.data_dirs.iter().map(|d| d.join("icons"))).collect();
        let unthemed = bases
            .iter()
            .cloned()
            .chain(env.data_dirs.iter().map(|d| d.join("pixmaps")))
            .chain(std::iter::once(PathBuf::from("/usr/share/pixmaps")))
            .collect();
        let theme = user_theme(env)
            .filter(|t| index(&bases, t).is_some())
            .unwrap_or_else(|| HICOLOR.to_string());
        let mut chain = Vec::new();
        collect(&bases, &theme, &mut chain);
        collect(&bases, HICOLOR, &mut chain);
        Resolver { theme, chain, unthemed }
    }

    /// The theme the chain started from.
    pub fn theme(&self) -> &str {
        &self.theme
    }

    pub fn resolve(&self, icon: &str, size: u32) -> Option<PathBuf> {
        let path = Path::new(icon);
        if path.is_absolute() {
            return is_icon_file(path).then(|| path.to_path_buf());
        }
        if icon.is_empty() || icon.contains('/') {
            return None;
        }
        self.chain.iter().find_map(|t| t.find(icon, size)).or_else(|| self.in_unthemed(icon))
    }

    fn in_unthemed(&self, name: &str) -> Option<PathBuf> {
        self.unthemed
            .iter()
            .flat_map(|dir| EXTENSIONS.iter().map(move |ext| dir.join(format!("{name}.{ext}"))))
            .find(|file| file.is_file())
    }
}

fn is_icon_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e)) && path.is_file()
}

fn user_theme(env: &Env) -> Option<String> {
    let text = std::fs::read_to_string(env.config_home.join("gtk-3.0/settings.ini")).ok()?;
    KeyFile::parse(&text).ok()?.string("Settings", "gtk-icon-theme-name").filter(|t| !t.is_empty())
}

fn index(bases: &[PathBuf], theme: &str) -> Option<PathBuf> {
    bases.iter().map(|b| b.join(theme).join("index.theme")).find(|p| p.is_file())
}

/// Appends `name` then its parents depth-first; a theme already in the chain is skipped.
fn collect(bases: &[PathBuf], name: &str, chain: &mut Vec<Theme>) {
    if chain.iter().any(|t| t.name == name) {
        return;
    }
    let Some(kf) = index(bases, name).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| KeyFile::parse(&t).ok())
    else {
        return;
    };
    let parents = kf.comma_list("Icon Theme", "Inherits").unwrap_or_default();
    chain.push(Theme { name: name.to_string(), dirs: subdirs(&kf, bases, name) });
    for parent in parents {
        collect(bases, &parent, chain);
    }
}

fn subdirs(kf: &KeyFile, bases: &[PathBuf], theme: &str) -> Vec<Subdir> {
    let listed = |key: &str| kf.comma_list("Icon Theme", key).unwrap_or_default();
    let number = |dir: &str, key: &str| kf.string(dir, key).and_then(|v| v.parse::<u32>().ok());
    listed("Directories")
        .into_iter()
        .chain(listed("ScaledDirectories"))
        .filter_map(|path| {
            let size = number(&path, "Size")?;
            if number(&path, "Scale").unwrap_or(1) != 1 {
                return None;
            }
            let kind = match kf.string(&path, "Type").as_deref() {
                Some("Fixed") => Kind::Fixed,
                Some("Scalable") => Kind::Scalable,
                _ => Kind::Threshold,
            };
            Some(Subdir {
                kind,
                size,
                min: number(&path, "MinSize").unwrap_or(size),
                max: number(&path, "MaxSize").unwrap_or(size),
                threshold: number(&path, "Threshold").unwrap_or(2),
                places: bases.iter().filter_map(|b| Place::list(b.join(theme).join(&path))).collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xdg::fixture::Scratch;

    fn index_theme(inherits: &str, dirs: &[(&str, &str)]) -> String {
        let names: Vec<_> = dirs.iter().map(|(n, _)| *n).collect();
        let mut text = format!("[Icon Theme]\nName=T\nInherits={inherits}\nDirectories={}\n", names.join(","));
        for (name, body) in dirs {
            text.push_str(&format!("\n[{name}]\n{body}\n"));
        }
        text
    }

    fn fixed(size: u32) -> (String, String) {
        (format!("{size}x{size}/apps"), format!("Size={size}\nType=Fixed"))
    }

    fn with_dirs(dirs: &[(String, String)]) -> String {
        let borrowed: Vec<(&str, &str)> = dirs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        index_theme("", &borrowed)
    }

    /// `fix` over `fixparent` over `hicolor`, spread across two base dirs.
    fn themes(s: &Scratch) -> Env {
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=fix\n");
        s.write(
            "one/icons/fix/index.theme",
            &index_theme("fixparent", &[("48x48/apps", "Size=48\nType=Fixed"), ("scalable/apps", "Size=64\nMinSize=16\nMaxSize=128\nType=Scalable")]),
        );
        s.write("two/icons/fixparent/index.theme", &index_theme("", &[("32x32/apps", "Size=32\nType=Fixed")]));
        s.write("two/icons/hicolor/index.theme", &index_theme("", &[("48x48/apps", "Size=48\nType=Fixed")]));
        s.env(&["one", "two"])
    }

    #[test]
    fn an_exact_fixed_match_wins() {
        let s = Scratch::new("xdg-icon-fixed");
        let env = themes(&s);
        let file = s.touch("one/icons/fix/48x48/apps/app.png");
        s.touch("two/icons/hicolor/48x48/apps/app.png");
        let r = Resolver::new(&env);
        assert_eq!(r.theme(), "fix");
        assert_eq!(r.resolve("app", 48), Some(file));
    }

    #[test]
    fn a_scalable_range_matches_inside_it() {
        let s = Scratch::new("xdg-icon-scalable");
        let env = themes(&s);
        let file = s.touch("one/icons/fix/scalable/apps/app.svg");
        let r = Resolver::new(&env);
        assert_eq!(r.resolve("app", 48), Some(file.clone()));
        assert_eq!(r.resolve("app", 16), Some(file.clone()));
        assert_eq!(r.resolve("app", 128), Some(file));
    }

    #[test]
    fn a_threshold_dir_matches_within_its_threshold() {
        let s = Scratch::new("xdg-icon-threshold");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=thr\n");
        s.write("share/icons/thr/index.theme", &index_theme("", &[("46/apps", "Size=46\nThreshold=2"), ("64/apps", "Size=64\nType=Threshold\nThreshold=1")]));
        let near = s.touch("share/icons/thr/46/apps/app.png");
        let far = s.touch("share/icons/thr/64/apps/other.png");
        let r = Resolver::new(&s.env(&["share"]));
        assert_eq!(r.resolve("app", 48), Some(near), "46 is within 2 of 48 (the default type is Threshold)");
        assert_eq!(r.resolve("other", 48), Some(far.clone()), "64 is outside 1 of 48: closest, not exact");
        assert_eq!(r.resolve("other", 63), Some(far));
    }

    #[test]
    fn the_smallest_size_at_or_above_beats_a_nearer_smaller_one() {
        let s = Scratch::new("xdg-icon-closest");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=sizes\n");
        s.write("share/icons/sizes/index.theme", &with_dirs(&[fixed(32), fixed(128), fixed(256)]));
        s.touch("share/icons/sizes/32x32/apps/app.png");
        let want = s.touch("share/icons/sizes/128x128/apps/app.png");
        s.touch("share/icons/sizes/256x256/apps/app.png");
        let r = Resolver::new(&s.env(&["share"]));
        assert_eq!(r.resolve("app", 48), Some(want), "32/128/256 at 48 picks 128");
    }

    #[test]
    fn with_only_smaller_sizes_the_largest_wins() {
        let s = Scratch::new("xdg-icon-smaller");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=sizes\n");
        s.write("share/icons/sizes/index.theme", &with_dirs(&[fixed(16), fixed(32), fixed(24)]));
        s.touch("share/icons/sizes/16x16/apps/app.png");
        let want = s.touch("share/icons/sizes/32x32/apps/app.png");
        s.touch("share/icons/sizes/24x24/apps/app.png");
        assert_eq!(Resolver::new(&s.env(&["share"])).resolve("app", 48), Some(want));
    }

    #[test]
    fn a_threshold_dir_ranks_by_its_range_and_a_tie_keeps_the_first() {
        let s = Scratch::new("xdg-icon-ranks");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=ranks\n");
        let dirs = [("t16", "Size=16"), ("t32", "Size=32"), ("t64", "Size=64"), ("t128", "Size=128"), ("s256", "Size=256\nMinSize=64\nMaxSize=256\nType=Scalable")];
        let dirs: Vec<_> = dirs.iter().map(|(n, b)| (format!("{n}/apps"), (*b).to_string())).collect();
        s.write("share/icons/ranks/index.theme", &with_dirs(&dirs));
        let place = |names: &[&str], icon: &str| {
            for n in names {
                s.touch(&format!("share/icons/ranks/{n}/apps/{icon}.png"));
            }
        };
        place(&["t16", "t128"], "far");
        place(&["t32", "t64"], "tied");
        place(&["t32", "t128", "s256"], "scalable");
        let r = Resolver::new(&s.env(&["share"]));
        assert_eq!(r.resolve("far", 48), Some(s.path("share/icons/ranks/t16/apps/far.png")), "30 below beats 78 above: Threshold has no larger-first rule");
        assert_eq!(r.resolve("tied", 48), Some(s.path("share/icons/ranks/t32/apps/tied.png")), "14 and 14: the first");
        assert_eq!(r.resolve("scalable", 48), Some(s.path("share/icons/ranks/t32/apps/scalable.png")), "14 beats a Scalable range 16 away");
    }

    #[test]
    fn a_fixed_size_below_ranks_after_any_other_kind_but_not_after_a_threshold_below() {
        let s = Scratch::new("xdg-icon-fixed-below");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=mix\n");
        let dirs = [("f24", "Size=24\nType=Fixed"), ("t16", "Size=16"), ("t128", "Size=128"), ("f32", "Size=32\nType=Fixed")];
        let dirs: Vec<_> = dirs.iter().map(|(n, b)| (format!("{n}/apps"), (*b).to_string())).collect();
        s.write("share/icons/mix/index.theme", &with_dirs(&dirs));
        s.touch("share/icons/mix/f24/apps/a.png");
        let t16 = s.touch("share/icons/mix/t16/apps/a.png");
        s.touch("share/icons/mix/f32/apps/b.png");
        let t128 = s.touch("share/icons/mix/t128/apps/b.png");
        let r = Resolver::new(&s.env(&["share"]));
        assert_eq!(r.resolve("a", 48), Some(t16), "Fixed 24 is below the request; Threshold 16 is merely far");
        assert_eq!(r.resolve("b", 48), Some(t128), "Fixed 32 is below the request, however near");
    }

    #[test]
    fn a_scalable_range_off_the_request_counts_by_its_nearest_end() {
        let s = Scratch::new("xdg-icon-range");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=rng\n");
        s.write(
            "share/icons/rng/index.theme",
            &index_theme("", &[("hi/apps", "Size=96\nMinSize=64\nMaxSize=512\nType=Scalable"), ("lo/apps", "Size=8\nMinSize=8\nMaxSize=24\nType=Scalable"), ("f/apps", "Size=96\nType=Fixed")]),
        );
        s.touch("share/icons/rng/hi/apps/hi.png");
        s.touch("share/icons/rng/f/apps/hi.png");
        let lo = s.touch("share/icons/rng/lo/apps/lo.svg");
        let r = Resolver::new(&s.env(&["share"]));
        assert_eq!(r.resolve("hi", 48), Some(s.path("share/icons/rng/hi/apps/hi.png")), "a range 16 away beats a fixed size 48 away");
        assert_eq!(r.resolve("lo", 48), Some(lo), "a range below the request is measured to its MaxSize");
    }

    #[test]
    fn extensions_are_tried_png_then_svg_then_xpm() {
        let s = Scratch::new("xdg-icon-ext");
        let env = themes(&s);
        s.touch("one/icons/fix/48x48/apps/app.xpm");
        s.touch("one/icons/fix/48x48/apps/app.svg");
        s.touch("one/icons/fix/48x48/apps/app.jpg");
        s.touch("one/icons/fix/48x48/apps/only.xpm");
        let r = Resolver::new(&env);
        assert_eq!(r.resolve("app", 48), Some(s.path("one/icons/fix/48x48/apps/app.svg")));
        assert_eq!(r.resolve("only", 48), Some(s.path("one/icons/fix/48x48/apps/only.xpm")));
        s.touch("one/icons/fix/48x48/apps/app.png");
        assert_eq!(Resolver::new(&env).resolve("app", 48), Some(s.path("one/icons/fix/48x48/apps/app.png")));
    }

    #[test]
    fn the_first_base_dir_wins_for_the_same_subdir() {
        let s = Scratch::new("xdg-icon-bases");
        let env = themes(&s);
        s.write("two/icons/fix/index.theme", &index_theme("", &[("48x48/apps", "Size=48\nType=Fixed")]));
        let first = s.touch("one/icons/fix/48x48/apps/app.png");
        s.touch("two/icons/fix/48x48/apps/app.png");
        assert_eq!(Resolver::new(&env).resolve("app", 48), Some(first));
    }

    #[test]
    fn a_theme_is_searched_before_its_parents_and_hicolor_last() {
        let s = Scratch::new("xdg-icon-chain");
        let env = themes(&s);
        let hicolor = s.touch("two/icons/hicolor/48x48/apps/app.png");
        assert_eq!(Resolver::new(&env).resolve("app", 48), Some(hicolor), "hicolor is the last resort");
        let parent = s.touch("two/icons/fixparent/32x32/apps/app.png");
        assert_eq!(Resolver::new(&env).resolve("app", 48), Some(parent), "a parent's 32 beats hicolor's exact 48");
        let own = s.touch("one/icons/fix/scalable/apps/app.svg");
        assert_eq!(Resolver::new(&env).resolve("app", 48), Some(own), "the theme itself beats both");
    }

    #[test]
    fn an_inherits_cycle_terminates() {
        let s = Scratch::new("xdg-icon-cycle");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=ping\n");
        s.write("share/icons/ping/index.theme", &index_theme("pong,hicolor", &[("48/apps", "Size=48\nType=Fixed")]));
        s.write("share/icons/pong/index.theme", &index_theme("ping", &[("48/apps", "Size=48\nType=Fixed")]));
        let want = s.touch("share/icons/pong/48/apps/app.png");
        let r = Resolver::new(&s.env(&["share"]));
        assert_eq!(r.resolve("app", 48), Some(want));
        assert_eq!(r.resolve("missing", 48), None);
    }

    #[test]
    fn loose_files_and_pixmaps_are_the_unthemed_fallback() {
        let s = Scratch::new("xdg-icon-loose");
        let env = themes(&s);
        let loose = s.touch("one/icons/loose.png");
        let pix = s.touch("two/pixmaps/pix.xpm");
        let r = Resolver::new(&env);
        assert_eq!(r.resolve("loose", 48), Some(loose));
        assert_eq!(r.resolve("pix", 48), Some(pix));
        s.touch("two/icons/hicolor/48x48/apps/loose.png");
        let themed = Resolver::new(&env).resolve("loose", 48).unwrap();
        assert!(themed.starts_with(s.path("two/icons/hicolor")), "a theme beats a loose file");
    }

    #[test]
    fn an_absolute_icon_must_be_an_existing_icon_file() {
        let s = Scratch::new("xdg-icon-abs");
        let r = Resolver::new(&themes(&s));
        let good = s.touch("anywhere/app.png");
        let bad_ext = s.touch("anywhere/app.jpg");
        assert_eq!(r.resolve(good.to_str().unwrap(), 48), Some(good));
        assert_eq!(r.resolve(bad_ext.to_str().unwrap(), 48), None);
        assert_eq!(r.resolve(s.path("anywhere/missing.png").to_str().unwrap(), 48), None);
        assert_eq!(r.resolve(s.path("anywhere").to_str().unwrap(), 48), None);
    }

    #[test]
    fn empty_and_slashed_relative_names_resolve_to_nothing() {
        let s = Scratch::new("xdg-icon-names");
        let env = themes(&s);
        s.touch("one/icons/fix/48x48/apps/app.png");
        let r = Resolver::new(&env);
        assert_eq!(r.resolve("", 48), None);
        assert_eq!(r.resolve("a/b", 48), None);
        assert_eq!(r.resolve("48x48/apps/app", 48), None);
    }

    #[test]
    fn without_a_usable_setting_the_theme_is_hicolor() {
        let s = Scratch::new("xdg-icon-default");
        s.write("share/icons/hicolor/index.theme", &index_theme("", &[("48/apps", "Size=48\nType=Fixed")]));
        let env = s.env(&["share"]);
        assert_eq!(Resolver::new(&env).theme(), "hicolor", "no settings.ini");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=not-installed\n");
        assert_eq!(Resolver::new(&env).theme(), "hicolor", "a theme no base dir holds");
        s.write("config/gtk-3.0/settings.ini", "not an ini file\n");
        assert_eq!(Resolver::new(&env).theme(), "hicolor", "an unreadable setting");
        s.write("home/.icons/mine/index.theme", &index_theme("", &[]));
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=mine\n");
        assert_eq!(Resolver::new(&env).theme(), "mine", "$HOME/.icons is a base dir");
    }

    #[test]
    fn scaled_dirs_and_scale_two_subdirs_are_read_at_scale_one_only() {
        let s = Scratch::new("xdg-icon-scale");
        s.write("config/gtk-3.0/settings.ini", "[Settings]\ngtk-icon-theme-name=hd\n");
        s.write(
            "share/icons/hd/index.theme",
            "[Icon Theme]\nName=hd\nDirectories=24/apps\nScaledDirectories=48@2/apps,48/apps\n\n\
             [24/apps]\nSize=24\nType=Fixed\n\n[48@2/apps]\nSize=48\nScale=2\nType=Fixed\n\n[48/apps]\nSize=48\nType=Fixed\n",
        );
        s.touch("share/icons/hd/48@2/apps/app.png");
        let want = s.touch("share/icons/hd/48/apps/app.png");
        assert_eq!(Resolver::new(&s.env(&["share"])).resolve("app", 48), Some(want));
    }
}
