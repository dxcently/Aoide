//! The pure `rice compose` scaffolding engine: name validation, the
//! Nix-literal renderer, and the `rice.nix`/`design/intent.md` templates.
//!
//! Moved out of `pkgs/aoide/src/commands/rice.rs` (Phase 5b restructure,
//! docs/architecture/PACKAGE-LAYOUT.md) — everything here is a pure function
//! of its inputs (no `Outcome`/`Invocation`, no filesystem I/O). The command
//! handler (`handle_rice_compose`, in `commands/rice.rs`) owns the I/O
//! (reading `--from`'s notes, writing the scaffolded files) and calls
//! through to this module for the actual rendering.

use crate::paths::STRILLIX_SHELL_SRC;
use serde_json::Value;

use crate::theme::schema::{POLARITY_DEFAULT, POLARITY_VALUES};

/// A valid `rice compose` rice name: `^[a-z0-9][a-z0-9-]*$`. This one check
/// also rejects path traversal (`..`, `/`) and case/underscore variance by
/// construction — nothing outside `[a-z0-9-]` is accepted, and the first
/// character can never be a `-`.
pub fn valid_rice_name(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit());
    first_ok && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Render one JSON scalar as a nix literal. The notes tiers `rice compose`
/// reads (palette / window / geometry) are leaves only — string, bool,
/// number, or null — so this never needs to handle arrays/objects.
pub fn nix_scalar(v: &Value) -> String {
    // ORDER MATTERS: backslash first (so the later escapes don't get
    // double-escaped), then the closing quote, then `$` — `\$` is the Nix
    // double-quoted-string escape that neutralizes `${…}` interpolation, so a
    // notes value like `"${builtins.readFile /etc/hostname}"` round-trips
    // into `rice.nix` as an inert literal, never live Nix interpolation.
    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
    }
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{}\"", escape(s)),
        other => format!("\"{}\"", escape(&other.to_string())),
    }
}

/// Render `key = value;` lines (one per line, `indent`-prefixed) for a FIXED
/// key list, pulling each value from `obj` (missing key → `null`). Used for
/// the window/geometry tiers so a partially-set source still yields the full
/// fixed key set — never a ragged subset a reader has to guess is exhaustive.
pub fn nix_fixed_fields(obj: Option<&serde_json::Map<String, Value>>, keys: &[&str], indent: &str) -> String {
    keys.iter()
        .map(|k| {
            let v = obj.and_then(|o| o.get(*k)).cloned().unwrap_or(Value::Null);
            format!("{indent}{k} = {};\n", nix_scalar(&v))
        })
        .collect()
}

/// The geometry tier's fixed key set, in the order CONTRACTS.md §1's table
/// lists them.
pub const GEOMETRY_KEYS: &[&str] = &[
    "gapsOut", "gapsIn", "borderSize", "rounding", "blurEnabled", "blurSize", "blurPasses",
    "terminalOpacity",
];
/// The window (border-colour) component tier's fixed key set.
pub const WINDOW_KEYS: &[&str] = &["border", "borderInactive"];

/// The ten component slots a rice's body selects, each filled by its `strillix`
/// provider (CONTRACTS.md §5). A scaffolded rice names all ten, as every
/// committed one does: a rice that paints a desktop paints all of it.
pub const STRILLIX_SLOTS: &[&str] = &[
    "bar",
    "launcher",
    "powermenu",
    "notifications",
    "wallpaper",
    "dock",
    "calendar",
    "switcher",
    "lock",
    "greeter",
];

/// Render one `rice.nix` for `rice compose`: a rice body (CONTRACTS.md §5,
/// rice shape v1) — the `system` half selects the ten `strillix` providers and
/// sets the theme copied from `notes` (palette, polarity, window, geometry),
/// the `home` half selects the `strillix` renderer and names this rice's
/// `hyprland.conf`. `from` is named in each block's comment, so a reader
/// tells an inherited value from a null fill template.
pub fn render_rice_nix(name: &str, from: &str, notes: &Value) -> String {
    let palette_lines = notes
        .get("palette")
        .and_then(Value::as_object)
        .map(|p| {
            p.iter()
                .map(|(k, v)| format!("        \"{k}\" = {};\n", nix_scalar(v)))
                .collect::<String>()
        })
        .unwrap_or_default();

    // The polarity field (CONTRACTS.md §1): a validated top-level scalar beside
    // `palette` — "a palette brings its polarity". Copied from the source
    // document when it carries one, and the option's own default otherwise, so
    // a composed rice declares its ground explicitly in the same key as its
    // palette rather than inheriting whatever the default becomes later.
    let declared = notes
        .get("polarity")
        .and_then(Value::as_str)
        .filter(|s| POLARITY_VALUES.contains(s));
    let polarity = declared.unwrap_or(POLARITY_DEFAULT);
    let polarity_comment = if declared.is_some() {
        format!("inherited from rice \"{from}\"")
    } else {
        format!(
            "\"{from}\" carries no polarity — the default ({POLARITY_DEFAULT}); \
             set \"dark\" if the copied palette is a dark ground"
        )
    };

    let window_obj = notes.get("window").and_then(Value::as_object);
    let window_lines = nix_fixed_fields(window_obj, WINDOW_KEYS, "        ");
    let window_comment = if window_obj.is_some() {
        format!("inherited from rice \"{from}\"")
    } else {
        format!("\"{from}\" set no window-colour overrides — null falls back to palette.accent/bg")
    };

    let geometry_obj = notes.get("geometry").and_then(Value::as_object);
    let geometry_lines = nix_fixed_fields(geometry_obj, GEOMETRY_KEYS, "        ");
    let geometry_comment = if geometry_obj.is_some() {
        format!("inherited from rice \"{from}\"")
    } else {
        format!(
            "fill template — \"{from}\" carries no geometry tier; null leaves the \
             host/compositor default (CONTRACTS.md §1)"
        )
    };

    let providers: String = STRILLIX_SLOTS
        .iter()
        .map(|slot| format!("      {slot} = \"strillix\";\n"))
        .collect();

    let mut s = String::new();
    s.push_str(&format!(
        "# rices/{name}/rice.nix — scaffolded via `strillix rice compose` from rice \"{from}\".\n"
    ));
    s.push_str("#\n");
    s.push_str("# A body (CONTRACTS.md §5): habit reads it only when this rice is performed\n");
    s.push_str("# or built in. The `system` half selects the paint; the `home` half the renderer.\n");
    s.push_str("# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): the rice sets ONLY strillix.theme\n");
    s.push_str("# and strillix.layout; its literal values are no reads of rice/ or of config.\n");
    s.push_str("{\n");
    s.push_str("  system = {\n");
    s.push_str("    providers = {\n");
    s.push_str(&providers);
    s.push_str("    };\n");
    s.push_str("    module = _: {\n");

    s.push_str("      strillix.theme.palette = {\n");
    s.push_str(&palette_lines);
    s.push_str("      };\n");

    s.push_str(&format!("\n      # polarity: light | dark — {polarity_comment}\n"));
    s.push_str(&format!("      strillix.theme.polarity = \"{polarity}\";\n"));

    s.push_str(&format!("\n      # {window_comment}\n"));
    s.push_str("      strillix.theme.window = {\n");
    s.push_str(&window_lines);
    s.push_str("      };\n");

    s.push_str(&format!("\n      # {geometry_comment}\n"));
    s.push_str("      strillix.theme.geometry = {\n");
    s.push_str(&geometry_lines);
    s.push_str("      };\n");

    s.push_str("    };\n");
    s.push_str("  };\n");
    s.push_str("  home = {\n");
    s.push_str("    members = [ \"strillix\" ];\n");
    s.push_str("    module = { rice.hyprland.conf = ./hyprland.conf; };\n");
    s.push_str("  };\n");
    s.push_str("}\n");
    s
}

/// The `hyprland.conf` a scaffolded rice starts from when `--from` ships none:
/// a header and no keys (`rice.hyprland.conf`, CONTRACTS.md §5). The binds stay
/// in `behaviour.nix`, so an empty file is the honest start.
pub fn render_hyprland_conf(name: &str) -> String {
    format!(
        "# rices/{name}/hyprland.conf — the rice's own binds, unbinds, layout and animations. No keys yet: behaviour.nix keeps the binds.\n"
    )
}

/// Render `design/intent.md` for `rice compose`: honest-empty — no
/// fabricated rationale, just what IS true (inherited from `from`, retune
/// it) and where to go to actually fill it in.
pub fn render_intent_md(name: &str, from: &str) -> String {
    format!(
        "# {name} — Design Intent\n\
         \n\
         **Rice:** {name}\n\
         **Palette/geometry:** inherited from `{from}` — retune\n\
         \n\
         ---\n\
         \n\
         ## Palette Rationale\n\
         \n\
         (not yet written — this rice was scaffolded from `{from}` via `strillix rice compose`, not designed)\n\
         \n\
         ## Component Tier\n\
         \n\
         (not yet written)\n\
         \n\
         ## Geometry\n\
         \n\
         (not yet written)\n\
         \n\
         ## Iteration Log\n\
         \n\
         ## How to fill this rice\n\
         \n\
         - Slot catalog (which slots a host wires today, what each expects): \
           `{STRILLIX_SHELL_SRC}/qml/slots.md`\n\
         - Per-rice widget contract: `CONTRACTS.md` §5, \"Per-rice flavor widgets\"\n\
         - Rices playbook: `rices/update-playbook.md`\n\
         - Drop a `widgets/<slot>.qml` here to dress a slot — any file under `widgets/` \
           becomes a slot named for its basename; nothing renders until a host surface \
           embeds a `WidgetSlot` anchor for that name.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nix_scalar_neutralizes_dollar_interpolation() {
        // `${` must never survive into the emitted literal live — `\$`
        // (backslash-then-quote-then-dollar ordering) is what makes a Nix
        // double-quoted string treat it as inert text.
        assert_eq!(
            nix_scalar(&Value::String("${builtins.currentTime}".to_string())),
            "\"\\${builtins.currentTime}\""
        );
        assert_eq!(
            nix_scalar(&Value::String("${x}".to_string())),
            "\"\\${x}\""
        );
        // Backslash-first ordering: a literal backslash ahead of `$` must not
        // get swallowed by the `$`-escape pass.
        assert_eq!(
            nix_scalar(&Value::String("\\${x}".to_string())),
            "\"\\\\\\${x}\""
        );
    }

    #[test]
    fn valid_rice_name_accepts_the_expected_shape() {
        for good in ["moonlight", "dusk2", "a", "rice-two-3"] {
            assert!(valid_rice_name(good), "`{good}` should be valid");
        }
        for bad in ["Dusk", "dusk_two", "-dusk", "dusk/two", "..", ""] {
            assert!(!valid_rice_name(bad), "`{bad}` should be invalid");
        }
    }
}
