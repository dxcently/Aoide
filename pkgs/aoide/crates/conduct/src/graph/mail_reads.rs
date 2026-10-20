//! The mail reads over the MCP door (MAIL.md "Reading over MCP"): the handlers
//! of `mail`, `mail read`, `mail show` and `mail mark`. `aoide-client` declares
//! the commands and their flags and answers the reads; this module decides what
//! a read may see. Off MCP the answer is the CLI's own, every mailbox and the
//! reader this process names. On MCP it is clipped to the caller's attested
//! session and the sessions it spawned, by the same walk the roster and
//! `session find` use ([`read_scope`]), and every read or mark acts as the
//! attested session's reader key alone.

use super::model::{load_stage, sessions_path, SessionRecord, SessionsFile};
use super::send::{read_scope, real_attested_sender};
use aoide_client::commands::MailScope;
use aoide_protocol::output::{Fix, Kind, Outcome};
use aoide_protocol::{Door, Invocation};

/// The sessions a read is clipped against: the stage's on MCP, none otherwise
/// (the CLI never consults them, so it never reads the stage for a mail read).
fn sessions_for(inv: &Invocation) -> Vec<SessionRecord> {
    if inv.door != Door::Mcp {
        return Vec::new();
    }
    load_stage::<SessionsFile>(&sessions_path()).unwrap_or_default().sessions
}

/// What a mail read may name, and the reader it reads as. `caller` names the
/// attested session of this process; an MCP read with none is refused, so an
/// unattested caller reaches no mailbox at all.
fn context_as(
    inv: &Invocation,
    sessions: &[SessionRecord],
    caller: impl Fn(&[SessionRecord]) -> Option<String> + Copy,
) -> Result<(MailScope, Option<String>), Outcome> {
    if inv.door != Door::Mcp {
        return Ok((None, aoide_client::commands::mail_reader_session()));
    }
    let Some(me) = caller(sessions) else {
        return Err(Outcome::refuse(
            inv.dotted(),
            Kind::Refused,
            "mail not read: this MCP server runs under no attested session",
            "an MCP read reaches only its own attested session's mailbox and the mailboxes of the sessions it spawned; an unattested server has none to reach",
            Fix::None("start the MCP server from a conducted session"),
        ));
    };
    let scope = read_scope(inv, sessions, |_| Some(me.clone()));
    Ok((scope, Some(me)))
}

/// `mail` (bare): the names this reader has unread mail under, within scope.
pub fn mail_names(inv: &Invocation) -> Outcome {
    match context_as(inv, &sessions_for(inv), real_attested_sender) {
        Ok((scope, reader)) => aoide_client::commands::mail_names(&scope, reader.as_deref()),
        Err(refusal) => refusal,
    }
}

/// `mail read`: the letters of one mailbox in scope, advancing the reader's mark.
pub fn mail_read(inv: &Invocation) -> Outcome {
    match context_as(inv, &sessions_for(inv), real_attested_sender) {
        Ok((scope, reader)) => aoide_client::commands::mail_read(inv, &scope, reader.as_deref()),
        Err(refusal) => refusal,
    }
}

/// `mail show`: one entry by msgid, when its mailbox is in scope.
pub fn mail_show(inv: &Invocation) -> Outcome {
    match context_as(inv, &sessions_for(inv), real_attested_sender) {
        Ok((scope, _)) => aoide_client::commands::mail_show(inv, &scope),
        Err(refusal) => refusal,
    }
}

/// `mail mark`: the reader's mark on one mailbox in scope.
pub fn mail_mark(inv: &Invocation) -> Outcome {
    match context_as(inv, &sessions_for(inv), real_attested_sender) {
        Ok((scope, reader)) => aoide_client::commands::mail_mark(inv, &scope, reader.as_deref()),
        Err(refusal) => refusal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::model::write_stage;
    use crate::graph::testutil::*;
    use aoide_protocol::registry::Registry;

    fn text_of(out: &Outcome) -> String {
        out.render(false).0
    }

    /// The caller is `me-id`; `kid-id` is attested under it; `sib-id` is not.
    fn family() -> Vec<SessionRecord> {
        let mut kid = session("kid-id", "/x", "idle", "2026-10-12T09:01:00Z", Some("me-id"));
        kid.attested_spawner = Some("me-id".into());
        vec![
            session("me-id", "/x", "working", "2026-10-12T09:00:00Z", None),
            kid,
            session("sib-id", "/x", "idle", "2026-10-12T09:02:00Z", None),
        ]
    }

    fn me(_: &[SessionRecord]) -> Option<String> {
        Some("me-id".to_string())
    }

    fn nobody(_: &[SessionRecord]) -> Option<String> {
        None
    }

    /// An isolated mail root with the family's stage written and letters filed
    /// to the caller's box, its subtree's, and a sibling's (two, so a msgid can
    /// be checked against the other).
    fn fixture(tag: &str) -> (EnvVars, std::path::PathBuf) {
        let guard = EnvVars::save(&["AOIDE_STATE_DIR", "AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);
        let root = unique_stage(tag);
        for sub in ["stage", "state", "state/mail", "state/sessions", "run"] {
            std::fs::create_dir_all(root.join(sub)).unwrap();
        }
        std::env::set_var("AOIDE_STAGE_DIR", root.join("stage"));
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("run"));
        write_stage(&sessions_path(), &SessionsFile { sessions: family(), ..Default::default() }).unwrap();
        aoide_storage::mail::file_letter("alice", "me-id", "for the caller").unwrap();
        aoide_storage::mail::file_letter("alice", "kid-id", "for the kid").unwrap();
        aoide_storage::mail::file_letter("alice", "sib-id", "for the sibling, first").unwrap();
        aoide_storage::mail::file_letter("alice", "sib-id", "for the sibling, second").unwrap();
        (guard, root)
    }

    fn msgid_in(name: &str, text: &str) -> String {
        aoide_storage::mail::read_base()
            .unwrap()
            .into_iter()
            .find(|e| e.envelope.header.to.name == name && e.envelope.text.contains(text))
            .map(|e| e.envelope.msgid)
            .unwrap()
    }

    fn mcp(path: &[&str], args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        Invocation {
            path: path.iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            door: Door::Mcp,
        }
    }

    /// Off MCP the context is the CLI's own: every mailbox, the env reader.
    #[test]
    fn a_cli_read_keeps_every_mailbox_and_its_reader() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-cli");
        let cli = flag_invocation(&["mail", "read"], &[("for", "sib-id")]);
        let (scope, _) = context_as(&cli, &family(), nobody).expect("the CLI is never refused here");
        assert!(scope.is_none(), "the CLI reads every mailbox");
    }

    /// The MCP clip is the caller's attested subtree: `kid-id` is in, `sib-id`
    /// is not, and an unattested caller is refused before any mailbox is named.
    #[test]
    fn an_mcp_read_is_clipped_to_the_attested_subtree() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-clip");
        let read = mcp(&["mail", "read"], &[], &[("for", "kid-id")]);
        let (scope, reader) = context_as(&read, &family(), me).expect("an attested caller");
        let scope = scope.expect("MCP is clipped");
        assert!(scope.contains("me-id") && scope.contains("kid-id"), "{scope:?}");
        assert!(!scope.contains("sib-id"), "a sibling is not the caller's: {scope:?}");
        assert_eq!(reader.as_deref(), Some("me-id"), "an MCP read acts as the attested session");

        let refused = context_as(&read, &family(), nobody).expect_err("an unattested MCP read is refused");
        assert!(text_of(&refused).contains("attested session"), "{}", text_of(&refused));
    }

    /// Naming a sibling's mailbox refuses with the same words as naming a
    /// mailbox that does not exist, so the refusal says nothing about either.
    #[test]
    fn an_out_of_scope_mailbox_refuses_as_an_absent_one_does() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-refuse");
        let sibling = mail_read_as(&mcp(&["mail", "read"], &[], &[("for", "sib-id")]));
        let absent = mail_read_as(&mcp(&["mail", "read"], &[], &[("for", "ghost-id")]));
        assert_eq!(sibling.status, absent.status);
        assert_eq!(sibling.message, absent.message, "a probe must learn nothing");
        assert!(!sibling.message.contains("first") && !sibling.message.contains("second"));
    }

    /// An MCP read moves only the attested reader's mark: another reader of the
    /// same mailbox still sees the letter unread.
    #[test]
    fn an_mcp_read_moves_only_the_attested_readers_mark() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-mark");
        aoide_storage::mail::enrol_reader("kid-id", "other-id").unwrap();
        let read = mcp(&["mail", "read"], &[], &[("for", "kid-id")]);
        let _ = mail_read_as(&read);
        let (_, other_unread) = aoide_storage::mail::unread_for("kid-id", Some("other-id")).unwrap();
        assert_eq!(other_unread.len(), 1, "another reader's mark was moved");
        let (_, own_unread) = aoide_storage::mail::unread_for("kid-id", Some("me-id")).unwrap();
        assert!(own_unread.is_empty(), "the attested reader's mark did not advance");
    }

    /// `mail show` of a sibling's msgid answers as an unknown msgid, and the
    /// teaching lists no msgid the caller cannot read.
    #[test]
    fn an_out_of_scope_msgid_is_unknown_and_unlisted() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-show");
        let hidden = msgid_in("sib-id", "first");
        let other = msgid_in("sib-id", "second");
        let show = mail_show_as(&mcp(&["mail", "show"], &[&hidden], &[]));
        assert_ne!(show.status, aoide_protocol::output::Status::Ok, "{}", show.message);
        assert!(!show.message.contains(&other), "the teaching listed another sibling msgid: {}", show.message);
        assert!(!show.message.contains("sibling"), "{}", show.message);
    }

    /// Mark refuses an out-of-scope mailbox and moves no cursor of it.
    #[test]
    fn an_out_of_scope_mark_is_refused_and_moves_nothing() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-mark-refused");
        let before = aoide_storage::mail::peek_cursors();
        let out = mail_mark_as(&mcp(&["mail", "mark"], &[], &[("for", "sib-id")]));
        assert_ne!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
        assert_eq!(before, aoide_storage::mail::peek_cursors(), "a refused mark moved a cursor");
    }

    /// The bare listing shows only the subtree's names with unread mail.
    #[test]
    fn the_mcp_bare_listing_names_only_the_subtree() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-names");
        let out = mail_names_as(&mcp(&["mail"], &[], &[]));
        let names = out.data.as_ref().expect("data")["names"].clone();
        let names: Vec<&str> = names.as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert!(names.contains(&"me-id") && names.contains(&"kid-id"), "{names:?}");
        assert!(!names.contains(&"sib-id"), "{names:?}");
    }

    /// A typo teaching names a mailbox that exists only out of scope: a scoped
    /// empty read gets no suggestion at all.
    #[test]
    fn a_scoped_read_never_suggests_a_mailbox_out_of_scope() {
        let _lock = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_guard, _root) = fixture("mail-reads-suggest");
        aoide_storage::mail::file_letter("alice", "kid-idx", "a sibling-side look-alike").unwrap();
        let out = mail_read_as(&mcp(&["mail", "read"], &[], &[("for", "kid-id")]));
        assert!(!text_of(&out).contains("did you mean"), "{}", text_of(&out));
    }

    /// The bare MCP listing and the reads are the four MCP-open commands; the
    /// `all-names` flag is CLI-only, so it can never widen an MCP read.
    #[test]
    fn the_read_commands_declare_the_mcp_door_and_all_names_stays_cli() {
        let mut r = Registry::new();
        crate::commands::graph::register_mail_names(&mut r);
        crate::commands::graph::register_mail_reads(&mut r);
        let paths: [&[&str]; 4] = [&["mail"], &["mail", "read"], &["mail", "show"], &["mail", "mark"]];
        for path in paths {
            let c = r.get(&path.iter().map(|s| s.to_string()).collect::<Vec<_>>()).expect("registered");
            assert!(c.opens(Door::Mcp), "{path:?} opens the MCP door");
        }
        let read = r.get(&["mail".to_string(), "read".to_string()]).unwrap();
        let all = read.flags.iter().find(|f| f.name == "all-names").unwrap();
        assert!(!all.doors.contains(&Door::Mcp), "--all-names is closed over MCP");
    }

    fn mail_read_as(inv: &Invocation) -> Outcome {
        match context_as(inv, &family(), me) {
            Ok((scope, reader)) => aoide_client::commands::mail_read(inv, &scope, reader.as_deref()),
            Err(refusal) => refusal,
        }
    }

    fn mail_show_as(inv: &Invocation) -> Outcome {
        match context_as(inv, &family(), me) {
            Ok((scope, _)) => aoide_client::commands::mail_show(inv, &scope),
            Err(refusal) => refusal,
        }
    }

    fn mail_mark_as(inv: &Invocation) -> Outcome {
        match context_as(inv, &family(), me) {
            Ok((scope, reader)) => aoide_client::commands::mail_mark(inv, &scope, reader.as_deref()),
            Err(refusal) => refusal,
        }
    }

    fn mail_names_as(inv: &Invocation) -> Outcome {
        match context_as(inv, &family(), me) {
            Ok((scope, reader)) => aoide_client::commands::mail_names(&scope, reader.as_deref()),
            Err(refusal) => refusal,
        }
    }
}
