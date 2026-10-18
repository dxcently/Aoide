//! `element seed` — full render of a rice's committed `elements/` directory
//! into `run/elements/` (docs/architecture/ELEMENTS.md, L-E1). The
//! shell-reachable bridge `crate::elements::seed_rice` backs: today it's run
//! by hand or by the User's own tooling; `rice stage` (L-E2) and the
//! elements lane's activation hook (L-E3) are later phases that call the
//! very same [`crate::elements::seed_rice`], not a fork of it.

use aoide_protocol::Invocation;
use aoide_protocol::output::Outcome;
use aoide_protocol::registry::{arg, cmd, Registry};
use serde_json::json;

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["element", "seed"],
        summary: "Render a rice's committed elements/*/element.json into run/elements/ — verbatim byte copy for template:false files, theme-rendered for template:true, atomic per file. A render error fails only that element, leaves its old config in place, and is reported per-element.",
        args: [arg!("rice", "string", true, "Rice to seed elements from.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_element_seed,
        brief: "Render a rice's elements into the run directory.",
    ));
}

fn handle_element_seed(inv: &Invocation) -> Outcome {
    let rice = match inv.args.first() {
        Some(n) => n.clone(),
        None => {
            return Outcome::usage("element.seed", "usage: strillix element seed <rice> [--json]")
                .with_data(json!({ "reason": "missing-rice" }));
        }
    };
    if !crate::compose::valid_rice_name(&rice) {
        return Outcome::error(
            "element.seed",
            format!(
                "`{rice}` is not a valid rice name: must match `^[a-z0-9][a-z0-9-]*$` \
                 (lowercase letters, digits, hyphens; no leading hyphen, no `/`, no `..`)"
            ),
        )
        .with_data(json!({ "reason": "invalid-name", "rice": rice }));
    }

    match crate::elements::seed_rice(&rice) {
        Ok(report) => {
            let total = report.elements.len();
            let failed = report.elements.iter().filter(|e| !e.ok).count();
            let data = json!({
                "rice": rice,
                "elements": report.elements,
            });
            if report.any_failed() {
                Outcome::error(
                    "element.seed",
                    format!("seeded {}/{total} element(s) for `{rice}` ({failed} failed)", total - failed),
                )
                .with_data(data)
            } else {
                Outcome::ok("element.seed", format!("seeded {total} element(s) for `{rice}`"))
                    .with_data(data)
            }
        }
        Err(e) => Outcome::error("element.seed", e.to_string())
            .with_data(json!({ "reason": "seed-failed", "rice": rice })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{unique_tmp, EnvSaver};

    #[test]
    fn seed_rejects_a_missing_rice_argument() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let out = handle_element_seed(&aoide_test_support::inv(&["element", "seed"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
    }

    #[test]
    fn seed_rejects_an_invalid_rice_name() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let out = handle_element_seed(&aoide_test_support::inv(&["element", "seed"], &["Bad Name"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Error);
        assert_eq!(out.data.unwrap()["reason"], "invalid-name");
    }

    #[test]
    fn seed_renders_a_rices_elements_into_run_elements() {
        // `rice_dir`/`run_elements_dir` both hang off `AOIDE_STAGE_DIR`
        // (`root/stage` → rice tree resolves at `root`) — mirrors
        // `commands/rice.rs`'s own test rig.
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        let root = unique_tmp("element-seed-cmd");
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));

        let strillix_dir = root.join("rices").join("moonlight");
        let elements_dir = strillix_dir.join("elements").join("waybar");
        std::fs::create_dir_all(&elements_dir).unwrap();
        std::fs::write(elements_dir.join("config.jsonc"), "{}").unwrap();
        std::fs::write(
            elements_dir.join("element.json"),
            r#"{ "v": 0, "element": "waybar", "files": [{"src":"config.jsonc"}], "run": {"exec":"waybar -c {run}/config.jsonc","via":"unit"} }"#,
        )
        .unwrap();
        std::fs::write(strillix_dir.join("theme.json"), aoide_test_support::VALID_NOTES).unwrap();

        let out = handle_element_seed(&aoide_test_support::inv(&["element", "seed"], &["moonlight"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{out:?}");
        let data = out.data.unwrap();
        assert_eq!(data["elements"][0]["element"], "waybar");
        assert_eq!(data["elements"][0]["ok"], true);

        let _ = std::fs::remove_dir_all(&root);
    }
}
