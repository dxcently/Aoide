# Session checklist

The mechanical motions of a working session in this repo, in order. The
rules themselves live in the root `AGENTS.md` (house rules, cited by number)
and the layered `AGENTS.md` files; this list only sequences them.

1. **Be a tracked session.** Every terminal here already runs conducted —
   registered in the session graph, conductable. Work launched outside a
   terminal wraps itself the same way: `aoide conduct -- <command>` (or
   `aoide spawn` for a detached launch). Never work as an untracked
   process.
2. **Write inside the domain.** Rice and theming work commits to
   `song/songbook/<song>/` and nothing else (house rule 1). Code work stays
   inside the directory whose `AGENTS.md` you have read — each layer's file
   states the invariants that bind you there.
3. **Test per crate.** `cargo test -p <crate>` for each crate touched —
   never `cargo test --workspace` on a live box
   (`pkgs/aoide/crates/AGENTS.md` states why: conduct and the server bind
   real sockets).
4. **Docs land in the same commit** (house rule 8). A change to a
   directory's seams, invariants, or extension points updates that
   directory's `README.md`/`AGENTS.md` in the commit that makes it — never
   a follow-up. Edit docs integrally (house rule 9): the page reads as if
   it was always so; the change record goes to the commit message and the
   log.
5. **Hand work back, gated.** You propose; the user admits; git records
   (house rule 2). No background rebuilds, no self-declared "done" — end by
   reporting what changed, what was verified, and what still needs the
   user's gate.
