//! `workspace set/clear/list` — the compositor's workspace ↔ project binding
//! (store, commands and resolution: core-seams design §B).
//!
//! A **workspace** is the compositor's own workspace id, an integer, exactly
//! the value `SessionRecord.workspace` already holds. A **binding** is
//! "workspace N shows project X": it is stored ON the project
//! (`Project.workspaces`), so `project remove` takes a binding with the
//! record it lives on, and a workspace id appears in at most one project —
//! `set` MOVES it. Two workspaces may show the same project.
//!
//! Composition: the binding is a `projects.json` mutation, so it is
//! DAEMON-OWNED exactly like the project mutations next door
//! ([`super::manage::local_daemon`]) — a CLI caller forwards to `aoided` and
//! errors when none answers. The one compositor-shaped half is resolving an
//! OMITTED `<workspace>`: that happens in the caller's process, before the
//! forward, because `aoided` is a service with no compositor environment —
//! the forwarded argv carries the resolved integer, never "focused".
//!
//! `list` is a read: no lock, no daemon, no stage write. It answers
//! honestly off a compositor-less host too — bindings still print, and
//! `"observed": false` says no session on this host reports a workspace (§A's
//! taught-refusal shape, without a refusal).

use super::common::{require_args, stage_error};
use super::doc::restage_graph;
use super::manage::local_daemon;
use super::model::{
    binding_for, load_stage, projects_path, sessions_path, write_stage, Project, ProjectsFile,
    SessionsFile, STAGE_GRAPH_VERSION,
};
use super::window::focused_workspace;
use aoide_protocol::output::Outcome;
use aoide_protocol::Invocation;
use aoide_storage::fs::with_stage_lock;
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// The taught refusal an omitted `<workspace>` gets when the compositor cannot
/// be asked. One text, both commands: the caller is told to give the number.
const NO_COMPOSITOR: &str =
    "no compositor adapter here to say which workspace is focused — give the workspace id";

/// One `<workspace>` argument as a workspace ID, or a taught refusal naming
/// the value. The key is an INTEGER, matching `SessionRecord.workspace`
/// (Hyprland's named and special workspaces carry negative ids, which parse
/// fine).
fn workspace_id(cmd: &str, raw: &str) -> Result<i64, Outcome> {
    raw.parse::<i64>().map_err(|_| {
        Outcome::usage(
            cmd,
            format!("`{raw}` is not a workspace id — a workspace is an integer (for example 3)"),
        )
        .with_data(json!({ "reason": "invalid-workspace", "workspace": raw }))
    })
}

/// `workspace set [<workspace>] <project>` — bind. The project must be
/// registered unless `--new` says to create it (name-only: a project may have
/// no folder), the id is MOVED off whatever project held it, and the binding
/// applies to sessions born afterwards (nothing is adopted retroactively).
pub fn workspace_set(inv: &Invocation) -> Outcome {
    // Two positionals are `<workspace> <project>`; one is `<project>`, bound to
    // the FOCUSED workspace. Resolve here, in the calling process, and forward
    // the RESOLVED argv — see the module doc.
    let (raw_ws, project) = match inv.args.as_slice() {
        [ws, project] => (ws.clone(), project.clone()),
        [project] => match focused_workspace() {
            Some(ws) => (ws.to_string(), project.clone()),
            None => {
                return Outcome::usage(
                    "workspace.set",
                    format!("{NO_COMPOSITOR}\nusage: aoide workspace set [<workspace>] <project>"),
                )
                .with_data(json!({ "reason": "no-compositor" }));
            }
        },
        _ => {
            return Outcome::usage(
                "workspace.set",
                "usage: aoide workspace set [<workspace>] <project> — with no workspace, the focused one is used",
            );
        }
    };
    let ws = match workspace_id("workspace.set", &raw_ws) {
        Ok(ws) => ws,
        Err(e) => return e,
    };
    let inv = Invocation {
        args: vec![ws.to_string(), project.clone()],
        ..inv.clone()
    };
    if let Some(out) = local_daemon(&inv) {
        return out;
    }
    bind_workspace(ws, &project, inv.flag_present("new"))
}

/// The local mutation behind `workspace set`, under ONE [`with_stage_lock`]
/// hold: the `--new` decision, the move off any previous project and the write
/// all happen under the same lock, so two threads racing `--new` for one name
/// can never both register it.
fn bind_workspace(ws: i64, project: &str, new: bool) -> Outcome {
    with_stage_lock(|| {
        let mut file: ProjectsFile = match load_stage(&projects_path()) {
            Ok(f) => f,
            Err(e) => return stage_error("workspace.set", e),
        };
        let mut changed: Vec<String> = Vec::new();

        if !file.projects.iter().any(|p| p.name == project) {
            if !new {
                return Outcome::error(
                    "workspace.set",
                    format!(
                        "no project named `{project}` is registered — register it with \
                         `project add {project}` (a folder is optional), or bind a new name-only \
                         project in one call with `workspace set {ws} {project} --new`"
                    ),
                )
                .with_data(json!({ "reason": "unknown-project", "project": project }));
            }
            file.projects.push(Project { name: project.to_string(), ..Default::default() });
            changed.push(format!("registered project {project} (no folder)"));
        }

        // A workspace id lives in at most one project: `set` MOVES it.
        for p in file.projects.iter_mut() {
            if p.name != project && p.workspaces.contains(&ws) {
                p.workspaces.retain(|w| *w != ws);
                changed.push(format!("workspace {ws} moved off project {}", p.name));
            }
        }
        let target = file.projects.iter_mut().find(|p| p.name == project).unwrap();
        if !target.workspaces.contains(&ws) {
            target.workspaces.push(ws);
            changed.push(format!("workspace {ws} → project {project}"));
        }

        if !changed.is_empty() {
            file.schema_version = STAGE_GRAPH_VERSION.to_string();
            file.projects.sort_by(|a, b| a.name.cmp(&b.name));
            if let Err(e) = write_stage(&projects_path(), &file) {
                return stage_error("workspace.set", e);
            }
            match restage_graph() {
                Ok(g) => changed.push(g.to_string_lossy().into_owned()),
                Err(e) => return stage_error("workspace.set", e),
            }
        }

        let message = if changed.is_empty() {
            format!("workspace {ws} is already bound to project `{project}` (no change)")
        } else {
            format!("bound workspace {ws} to project `{project}`")
        };
        Outcome::ok("workspace.set", message)
            .changed(changed)
            .with_data(json!({
                "workspace": ws,
                "project": project,
                "file": projects_path().to_string_lossy(),
            }))
    })
}

/// `workspace clear <workspace>` — unbind. Sessions ALREADY stamped on that
/// workspace keep the default project they were born with: the binding is a
/// label plus a birth default, never a live link, so nothing is re-resolved.
pub fn workspace_clear(inv: &Invocation) -> Outcome {
    if let Some(out) = local_daemon(inv) {
        return out;
    }
    let args = match require_args(inv, &["workspace"]) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let ws = match workspace_id("workspace.clear", &args[0]) {
        Ok(ws) => ws,
        Err(e) => return e,
    };
    unbind_workspace(ws)
}

/// The local mutation behind `workspace clear`, under ONE lock hold.
fn unbind_workspace(ws: i64) -> Outcome {
    with_stage_lock(|| {
        let mut file: ProjectsFile = match load_stage(&projects_path()) {
            Ok(f) => f,
            Err(e) => return stage_error("workspace.clear", e),
        };
        let mut changed: Vec<String> = Vec::new();
        let mut was: Option<String> = None;
        for p in file.projects.iter_mut() {
            if p.workspaces.contains(&ws) {
                p.workspaces.retain(|w| *w != ws);
                was = Some(p.name.clone());
                changed.push(format!("workspace {ws} unbound from project {}", p.name));
            }
        }
        if changed.is_empty() {
            return Outcome::ok(
                "workspace.clear",
                format!("workspace {ws} is not bound to any project (no change)"),
            )
            .with_data(json!({ "workspace": ws }));
        }
        file.schema_version = STAGE_GRAPH_VERSION.to_string();
        if let Err(e) = write_stage(&projects_path(), &file) {
            return stage_error("workspace.clear", e);
        }
        match restage_graph() {
            Ok(g) => changed.push(g.to_string_lossy().into_owned()),
            Err(e) => return stage_error("workspace.clear", e),
        }
        Outcome::ok(
            "workspace.clear",
            format!(
                "unbound workspace {ws} (was project `{}`)",
                was.unwrap_or_default()
            ),
        )
        .changed(changed)
        .with_data(json!({ "workspace": ws, "file": projects_path().to_string_lossy() }))
    })
}

/// `workspace list [--json]` — every BINDING plus every workspace a session on
/// this host reports, sorted by id. Read-only: no lock, no daemon. A host with
/// no compositor still lists its bindings and says `"observed": false` by
/// name, so the answer is honest rather than empty-looking.
pub fn workspace_list(_inv: &Invocation) -> Outcome {
    let file: ProjectsFile = match load_stage(&projects_path()) {
        Ok(f) => f,
        Err(e) => return stage_error("workspace.list", e),
    };
    let sessions: SessionsFile = match load_stage(&sessions_path()) {
        Ok(f) => f,
        Err(e) => return stage_error("workspace.list", e),
    };
    let observed: BTreeSet<i64> = sessions.sessions.iter().filter_map(|s| s.workspace).collect();
    let mut ids: BTreeSet<i64> = observed.clone();
    for p in &file.projects {
        ids.extend(p.workspaces.iter().copied());
    }
    let rows: Vec<Value> = ids
        .iter()
        .map(|ws| match binding_for(*ws, &file.projects) {
            Some(i) => json!({ "workspace": ws, "project": file.projects[i].name }),
            None => json!({ "workspace": ws }),
        })
        .collect();
    let bound = rows.iter().filter(|r| r.get("project").is_some()).count();

    let mut message = if rows.is_empty() {
        "no workspace is bound or observed".to_string()
    } else {
        format!("{} workspace(s), {bound} bound", rows.len())
    };
    for r in &rows {
        message.push_str(&format!(
            "\n  {}  {}",
            r["workspace"],
            r.get("project").and_then(Value::as_str).unwrap_or("(unbound)")
        ));
    }
    if observed.is_empty() {
        message.push_str("\n(no session on this host reports a workspace)");
    }

    let mut data = json!({ "observed": !observed.is_empty(), "workspaces": rows });
    if observed.is_empty() {
        data["reason"] = json!("no session on this host reports a workspace");
    }
    Outcome::ok("workspace.list", message).with_data(data)
}

/// `workspace root [<workspace>]` — the FIRST folder of the project bound to
/// the given (or FOCUSED) workspace, for a launcher that opens a new terminal
/// in the right directory:
///
/// ```text
/// kitty --directory "$(aoide workspace root 2>/dev/null || echo "$HOME")"
/// ```
///
/// Read-only: no lock, no daemon, no write. The success message IS the path,
/// alone, because the CLI door prints it raw for exactly that substitution
/// (`cli`'s own `special` arm) — nothing else goes to stdout on success, and
/// NOTHING goes to stdout on failure, where the exit code is the whole answer
/// (no binding, no folder, or no workspace given and no compositor to ask).
pub fn workspace_root(inv: &Invocation) -> Outcome {
    let ws = match inv.args.as_slice() {
        [] => match focused_workspace() {
            Some(ws) => ws,
            None => {
                return Outcome::usage(
                    "workspace.root",
                    format!("{NO_COMPOSITOR}\nusage: aoide workspace root [<workspace>]"),
                )
                .with_data(json!({ "reason": "no-compositor" }));
            }
        },
        [raw] => match workspace_id("workspace.root", raw) {
            Ok(ws) => ws,
            Err(e) => return e,
        },
        _ => {
            return Outcome::usage(
                "workspace.root",
                "usage: aoide workspace root [<workspace>] — with no workspace, the focused one is used",
            );
        }
    };

    let file: ProjectsFile = match load_stage(&projects_path()) {
        Ok(f) => f,
        Err(e) => return stage_error("workspace.root", e),
    };
    let Some(i) = binding_for(ws, &file.projects) else {
        return Outcome::error(
            "workspace.root",
            format!("workspace {ws} is not bound to any project"),
        )
        .with_data(json!({ "reason": "no-binding", "workspace": ws }));
    };
    let project = &file.projects[i];
    let Some(root) = project.roots().first().copied() else {
        return Outcome::error(
            "workspace.root",
            format!(
                "project `{}` has no folder — add one with `project add {} <root>`",
                project.name, project.name
            ),
        )
        .with_data(json!({
            "reason": "no-folder",
            "workspace": ws,
            "project": project.name,
        }));
    };
    // The message is the path and nothing else: the CLI door substitutes it.
    Outcome::ok("workspace.root", root).with_data(json!({
        "workspace": ws,
        "project": project.name,
        "root": root,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::*;
    use aoide_protocol::Door;

    fn daemon_invocation(path: &[&str], args: &[&str]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: Default::default(),
            door: Door::Daemon,
        }
    }

    /// A stage with one rooted project (`aoide`) and one name-only project
    /// (`cadenza`), both registered through the real `project add` handler.
    fn two_projects(stage: &std::path::Path) -> String {
        let root = stage.join("aoide-root");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.to_string_lossy().into_owned();
        crate::graph::project_add(&daemon_invocation(&["project", "add"], &["aoide", &root]));
        crate::graph::project_add(&daemon_invocation(&["project", "add"], &["cadenza"]));
        root
    }

    #[test]
    fn set_binds_a_workspace_and_moves_it_off_its_previous_project() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("ws-set-moves");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        two_projects(&stage);

        let out = workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "aoide"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert_eq!(
            file.projects.iter().find(|p| p.name == "aoide").unwrap().workspaces,
            vec![3]
        );

        // Two workspaces may show the same project.
        workspace_set(&daemon_invocation(&["workspace", "set"], &["5", "aoide"]));
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert_eq!(
            file.projects.iter().find(|p| p.name == "aoide").unwrap().workspaces,
            vec![3, 5]
        );

        // …but ONE workspace lives in exactly one project: `set` moves it.
        let out = workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "cadenza"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert!(
            out.changed.iter().any(|c| c.contains("workspace 3 moved off project aoide")),
            "the move is reported: {:?}",
            out.changed
        );
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert_eq!(
            file.projects.iter().find(|p| p.name == "aoide").unwrap().workspaces,
            vec![5]
        );
        assert_eq!(
            file.projects.iter().find(|p| p.name == "cadenza").unwrap().workspaces,
            vec![3],
            "a NAME-ONLY project binds fine — a binding is not a folder"
        );
        assert_eq!(binding_for(3, &file.projects).map(|i| file.projects[i].name.clone()), Some("cadenza".into()));

        // Re-binding the same pair is a no-op, not a duplicate.
        let out = workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "cadenza"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        assert!(out.changed.is_empty(), "idempotent: {:?}", out.changed);
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert_eq!(
            file.projects.iter().find(|p| p.name == "cadenza").unwrap().workspaces,
            vec![3]
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn set_refuses_an_unregistered_project() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("ws-set-unknown");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        two_projects(&stage);

        let out = workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "typo"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Error);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "unknown-project");
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert!(
            !file.projects.iter().any(|p| p.name == "typo"),
            "a typo never registers a project and never binds"
        );
        assert!(file.projects.iter().all(|p| !p.workspaces.contains(&3)));

        // `--new` is the explicit way to mean it, and creates it name-only.
        let mut inv = daemon_invocation(&["workspace", "set"], &["3", "typo"]);
        inv.flags.insert("new".into(), "true".into());
        let out = workspace_set(&inv);
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        let p = file.projects.iter().find(|p| p.name == "typo").unwrap();
        assert!(p.roots().is_empty(), "created name-only: {:?}", p.roots());
        assert_eq!(p.workspaces, vec![3]);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn an_omitted_workspace_needs_a_compositor_and_never_guesses() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        let stage = unique_stage("ws-set-focused-none");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        two_projects(&stage);

        let out = workspace_set(&daemon_invocation(&["workspace", "set"], &["aoide"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "no-compositor");
        assert!(
            out.message.contains("give the workspace id"),
            "taught, asking for the number: {}",
            out.message
        );
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert!(
            file.projects.iter().all(|p| p.workspaces.is_empty()),
            "nothing is bound without the id"
        );

        // A non-integer id is refused by name, not silently coerced.
        let out = workspace_set(&daemon_invocation(&["workspace", "set"], &["three", "aoide"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "invalid-workspace");

        match saved_sig {
            Some(v) => std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v),
            None => std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn clear_unbinds_and_leaves_the_project_and_stamped_sessions_alone() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("ws-clear");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let root = two_projects(&stage);
        workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "aoide"]));

        let out = workspace_clear(&daemon_invocation(&["workspace", "clear"], &["3"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert!(out.message.contains("was project `aoide`"), "{}", out.message);
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        let p = file.projects.iter().find(|p| p.name == "aoide").unwrap();
        assert!(p.workspaces.is_empty(), "the binding is gone");
        assert_eq!(p.roots(), vec![root.as_str()], "the project itself is untouched");

        // Clearing again is an ok no-op.
        let out = workspace_clear(&daemon_invocation(&["workspace", "clear"], &["3"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        assert!(out.changed.is_empty(), "no-op: {:?}", out.changed);

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn project_remove_takes_its_bindings_and_a_root_removal_does_not() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("ws-project-remove");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let root = two_projects(&stage);
        workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "aoide"]));
        workspace_set(&daemon_invocation(&["workspace", "set"], &["5", "cadenza"]));

        // Removing a ROOT leaves the project (and its bindings) standing.
        crate::graph::project_remove(&daemon_invocation(
            &["project", "remove"],
            &["aoide", &root],
        ));
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        let p = file.projects.iter().find(|p| p.name == "aoide").unwrap();
        assert!(p.roots().is_empty(), "now name-only");
        assert_eq!(p.workspaces, vec![3], "a binding is not a folder");

        // The bare form deletes the project, bindings and all.
        crate::graph::project_remove(&daemon_invocation(&["project", "remove"], &["aoide"]));
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert!(!file.projects.iter().any(|p| p.name == "aoide"));
        assert_eq!(binding_for(3, &file.projects), None, "the binding went with it");
        assert_eq!(
            binding_for(5, &file.projects).map(|i| file.projects[i].name.clone()),
            Some("cadenza".into()),
            "another project's binding is untouched"
        );

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn list_shows_bindings_and_observed_workspaces_with_an_honest_observed_flag() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("ws-list");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        two_projects(&stage);
        workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "aoide"]));

        // A session on workspace 7 (unbound) and one with no workspace at all.
        let mut on_seven = session("s7", "/w", "working", "t", None);
        on_seven.workspace = Some(7);
        let no_ws = session("s0", "/w", "working", "t", None);
        write_stage(
            &sessions_path(),
            &SessionsFile {
                schema_version: "0".into(),
                sessions: vec![on_seven, no_ws],
            },
        )
        .unwrap();

        let out = workspace_list(&daemon_invocation(&["workspace", "list"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        let rows = out.data.as_ref().unwrap()["workspaces"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 2, "the binding plus the observed id: {rows:?}");
        assert_eq!(rows[0]["workspace"], 3);
        assert_eq!(rows[0]["project"], "aoide");
        assert_eq!(rows[1]["workspace"], 7);
        assert!(rows[1].get("project").is_none(), "unbound carries no project");
        assert_eq!(out.data.as_ref().unwrap()["observed"], true);
        assert!(
            out.message.contains("3  aoide") && out.message.contains("7  (unbound)"),
            "{}",
            out.message
        );

        // No session reports a workspace → bindings still print, and the flag
        // says by name why nothing was observed.
        write_stage(
            &sessions_path(),
            &SessionsFile { schema_version: "0".into(), sessions: vec![] },
        )
        .unwrap();
        let out = workspace_list(&daemon_invocation(&["workspace", "list"], &[]));
        assert_eq!(out.data.as_ref().unwrap()["observed"], false);
        assert_eq!(
            out.data.as_ref().unwrap()["reason"],
            "no session on this host reports a workspace"
        );
        let rows = out.data.as_ref().unwrap()["workspaces"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "a binding survives an empty roster: {rows:?}");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn root_prints_the_first_folder_of_the_bound_project() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("ws-root");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let root = two_projects(&stage);
        // A SECOND folder on the same project: `root` answers with the FIRST.
        let second = stage.join("aoide-second");
        std::fs::create_dir_all(&second).unwrap();
        let second = second.to_string_lossy().into_owned();
        crate::graph::project_add(&daemon_invocation(
            &["project", "add"],
            &["aoide", &second],
        ));
        workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "aoide"]));

        let out = workspace_root(&daemon_invocation(&["workspace", "root"], &["3"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert_eq!(out.message, root, "the message IS the bare path");
        assert_eq!(out.data.as_ref().unwrap()["root"], root);
        assert_eq!(out.data.as_ref().unwrap()["project"], "aoide");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn root_refuses_an_unbound_workspace_and_a_project_with_no_folder() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("ws-root-refusals");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        two_projects(&stage);

        // Unbound: no binding to answer with.
        let out = workspace_root(&daemon_invocation(&["workspace", "root"], &["3"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Error);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "no-binding");
        assert_ne!(out.status.exit_code(), 0, "a launcher's `||` must fire");
        assert!(
            out.data.as_ref().unwrap().get("root").is_none(),
            "no path is ever published for a refusal"
        );

        // Bound, but the project is NAME-ONLY: no folder to print.
        workspace_set(&daemon_invocation(&["workspace", "set"], &["5", "cadenza"]));
        let out = workspace_root(&daemon_invocation(&["workspace", "root"], &["5"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Error);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "no-folder");
        assert!(out.message.contains("has no folder"), "{}", out.message);
        assert_ne!(out.status.exit_code(), 0);

        // A non-integer id is refused by name.
        let out = workspace_root(&daemon_invocation(&["workspace", "root"], &["three"]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "invalid-workspace");

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn root_with_no_workspace_needs_the_compositor_and_never_guesses() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        let stage = unique_stage("ws-root-focused");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        two_projects(&stage);
        workspace_set(&daemon_invocation(&["workspace", "set"], &["3", "aoide"]));

        let out = workspace_root(&daemon_invocation(&["workspace", "root"], &[]));
        assert_eq!(out.status, aoide_protocol::output::Status::Usage);
        assert_eq!(out.data.as_ref().unwrap()["reason"], "no-compositor");
        assert_ne!(out.status.exit_code(), 0);
        assert!(out.message.contains("give the workspace id"), "{}", out.message);

        match saved_sig {
            Some(v) => std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", v),
            None => std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }

    #[test]
    fn the_forwarded_argv_carries_the_resolved_workspace_not_the_word_focused() {
        // ONE positional is the PROJECT: the workspace comes from the
        // compositor adapter, in the CALLER's process. What the daemon
        // receives must therefore be the RESOLVED INTEGER — `aoided` is a
        // service with no compositor environment, so "focused" can never cross
        // the wire. A stub daemon stands at `AOIDE_DAEMON_SOCKET` and hands
        // back the request it was given.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = EnvVars::save(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET"]);
        let stage = unique_stage("ws-forward");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        two_projects(&stage);

        let (shim, dir) = fake_hyprctl("ws-forward");
        std::env::set_var("AOIDE_TEST_WS", "5");

        let sock = stage.join("d.sock");
        let listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        std::env::set_var("AOIDE_DAEMON_SOCKET", &sock);
        let stub = std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            let (conn, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(conn);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let reply = serde_json::json!({
                "outcome": Outcome::ok("workspace.set", "bound by the stub daemon")
            })
            .to_string()
                + "\n";
            reader.into_inner().write_all(reply.as_bytes()).unwrap();
            line
        });

        let mut inv = daemon_invocation(&["workspace", "set"], &["aoide"]);
        inv.door = Door::Cli;
        inv.flags.insert("new".into(), "true".into());
        let out = workspace_set(&inv);
        let line = stub.join().unwrap();
        let req: serde_json::Value = serde_json::from_str(line.trim()).unwrap();

        assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert_eq!(req["path"], serde_json::json!(["workspace", "set"]));
        assert_eq!(
            req["args"],
            serde_json::json!(["5", "aoide"]),
            "the forwarded argv carries the RESOLVED integer, never the word `focused`: {}",
            req["args"]
        );
        assert_eq!(req["flags"]["new"], "true", "flags ride along with the resolved argv");

        // …and the Door::Cli caller wrote nothing locally: the daemon owns it.
        let file: ProjectsFile = load_stage(&projects_path()).unwrap();
        assert!(file.projects.iter().all(|p| p.workspaces.is_empty()));

        drop(shim);
        let _ = std::fs::remove_dir_all(&dir);
        drop(saved);
        let _ = std::fs::remove_dir_all(&stage);
    }
}
