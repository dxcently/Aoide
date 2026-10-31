//! A hook-named Claude transcript is recorded only when it passes every check,
//! one test per check.

use aoide_protocol::agents::{claude_transcript_refusal, transcript_refusal, CLAUDE_TRANSCRIPT_CHECKS};
use std::path::{Path, PathBuf};

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ao-admit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("projects").join("-home-u-Aoide")).unwrap();
    root
}

fn own_uid() -> u32 {
    unsafe { libc::geteuid() as u32 }
}

#[test]
fn the_checks_are_four_and_named_once() {
    let names: Vec<&str> = CLAUDE_TRANSCRIPT_CHECKS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        ["not-a-regular-file", "not-owned-by-this-user", "name-is-not-the-session", "not-under-projects"]
    );
}

#[test]
fn a_symlink_to_a_real_transcript_is_refused_by_the_regular_file_check() {
    let root = scratch("link");
    let bucket = root.join("projects").join("-home-u-Aoide");
    let real = root.join("real.jsonl");
    std::fs::write(&real, "{}\n").unwrap();
    let link = bucket.join("sid-1.jsonl");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert_eq!(claude_transcript_refusal(&link, "sid-1"), Some("not-a-regular-file"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_owned_by_another_uid_is_refused_by_the_owner_check() {
    let root = scratch("owner");
    let file = root.projects_file("sid-2");
    std::fs::write(&file, "{}\n").unwrap();
    assert_eq!(transcript_refusal(&file, "sid-2", own_uid() + 1), Some("not-owned-by-this-user"));
    assert_eq!(transcript_refusal(&file, "sid-2", own_uid()), None, "the same file, its own owner");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_transcript_named_for_another_session_is_refused_by_the_name_check() {
    let root = scratch("name");
    let other = root.projects_file("other");
    std::fs::write(&other, "{}\n").unwrap();
    assert_eq!(claude_transcript_refusal(&other, "sid-3"), Some("name-is-not-the-session"));
    assert!(claude_transcript_refusal(Path::new("/etc/passwd"), "sid-3").is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_path_outside_projects_or_with_dot_dot_is_refused_by_the_projects_check() {
    let root = scratch("projects");
    std::fs::create_dir_all(root.join("elsewhere").join("bkt")).unwrap();
    let outside = root.join("elsewhere").join("bkt").join("sid-4.jsonl");
    std::fs::write(&outside, "{}\n").unwrap();
    assert_eq!(claude_transcript_refusal(&outside, "sid-4"), Some("not-under-projects"));
    let dotdot = root.join("projects").join("-home-u-Aoide").join("..").join("sid-4.jsonl");
    std::fs::write(root.join("projects").join("sid-4.jsonl"), "{}\n").unwrap();
    assert_eq!(claude_transcript_refusal(&dotdot, "sid-4"), Some("not-under-projects"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_transcript_under_projects_with_its_session_name_is_admitted() {
    let root = scratch("admit");
    let file = root.projects_file("sid-5");
    std::fs::write(&file, "{}\n").unwrap();
    assert_eq!(claude_transcript_refusal(&file, "sid-5"), None);
    let _ = std::fs::remove_dir_all(&root);
}

trait ProjectsFile {
    fn projects_file(&self, sid: &str) -> PathBuf;
}

impl ProjectsFile for PathBuf {
    fn projects_file(&self, sid: &str) -> PathBuf {
        self.join("projects").join("-home-u-Aoide").join(format!("{sid}.jsonl"))
    }
}
