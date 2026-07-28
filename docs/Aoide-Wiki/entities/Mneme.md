---
type: entity
created: 2026-07-26
updated: 2026-07-28
aliases: [Mneme Vault Server, mneme.service]
tags: [aoide, mneme, knowledge, vault, mcp, content]
---

# Mneme

The knowledge server AoideOS **integrates** — the **door**. Mneme is an
independently-owned vault API for the client that has no shell (the repo ships
the launcher `modules/dendrites/mneme.nix`, not Mneme's source). It serves a
folder of text notes over MCP: read, write, snapshot, search, and serve
conventions and skills — CRUD over a directory, with version history and trash.
In AoideOS it is the substrate behind the [[Content-Pipeline]] and the
[[Wiki-Protocol]].

**The muse.** Mneme = **memory**, the counterpart to [[Melete]] = practice and
**Aoide** = song (the three Boeotian Muses). Practice *acts*; memory *persists
what practice produced*; the song is *performed* on top. Aoide is the third muse
that completes the triad.

## The one governing fact — doer vs. door

Two clients reach for a vault, and not the same way. **Melete has a shell and
owns the disk**, so it barely needs a vault API — it writes raw. **A shell-less
agent** (Claude on an MCP connector) has no other reach, so Mneme is *its* only
entrance. That asymmetry is the whole reason Mneme is a **separate daemon** rather
than a Melete module:

- The vault has a **second reader** — keeping it apart from the harness keeps
  *read-my-notes* and *run-code-on-my-repos* two separate keys, not one
  over-powered key.
- The **doer is volatile** (self-updates, spawns agents, long-running jobs);
  **memory must not share its fate** — kept apart, the harness can thrash while
  the vault holds steady.
- **Separate processes, separate placement** — one memory, many hands; the single
  source of truth doesn't splinter into per-host copies.

## What it is

- **An MCP server** (`mneme.service`), independent of Melete — its own systemd
  unit, working dir, and auth. Binds `127.0.0.1:8000`, serves `/mcp`. The Mneme
  connector is the vault door — distinct from the [[Melete]] connector (the
  doer harness) and the Aoide connector (the desktop/system frame's own
  management surface). See [[Agent-Interface]].
- **Auth:** an operator passphrase is exchanged for a short-lived OAuth token,
  presented to `/mcp`. Mneme mints and validates it.
- **Versioning & trash:** edits routed through Mneme's API produce a version
  snapshot (50 per note) and deletes go to trash — recoverable. **Raw shell
  writes bypass this**, so the guarantee covers exactly the client that has no
  other way in (the shell-less reader).
- **Passive sync:** Mneme serves and versions what's on disk; an external daemon
  (Syncthing) propagates across devices — not Mneme's job.

## Feature set

| Chain | Tools |
|---|---|
| **Read** | `read_note`, `list_notes`, `get_index`, `search_vault`, `read_canvas` |
| **Write** | `create_note`, `update_note`, `edit_note` (exact-substring), `replace_section`, `append_to_note`, `create_canvas` |
| **Delete / recover** | `delete_note` (→ trash), `list_trash`, `restore_note`, `empty_trash` |
| **Versions** | `list_versions`, `read_version`, `restore_version` |
| **Conventions & skills** | `get_conventions`, `read_convention` (the operating rules a KB expects); `skill_body` (fetch a skill's canonical `SKILL.md`) |

Notes aren't limited to markdown — any UTF-8 text is listed, readable, and
writable; `.canvas` files get dedicated tools. Every operation is confined to the
vault root.

## Its part in the Aoide system

- **Behind the content pipeline.** Aoide reads a vault **exclusively via Mneme's
  API** — raw filesystem access to a vault is never permitted. The vault declares
  its exports; Aoide admits what it wants through the [[Content-Pipeline]]
  **approve gate**. This is a deliberate injection-defense seam.
- **The keeper of project wikis.** The [[Wiki-Protocol]] is owned/managed by
  Mneme (with [[Melete]]) — Mneme mints and keeps each project's standalone wiki
  in one shared shape. Aoide's own wiki is the self-managed exception.
- **The "door" pattern, generalized.** Where a capability must be reached by a
  shell-less agent, Mneme is the model: a narrow, authenticated, versioned API
  rather than raw disk access — the same discipline Aoide applies to
  [[Agent-Interface|its own CLI/MCP surface]].
- **Conventions & skills** feed the agent the operating rules and reusable skills
  it should follow, served the same way notes are.

*Source: the Magi GNOSIS `Melete-Mneme` sub-wiki (Overview, Melete-Mneme-Interaction,
Vault-Access), read via the `Mneme:Sakaki` connector, plus the live Mneme MCP tool
surface.*

## Related

- [[Melete]]
- [[Content-Pipeline]]
- [[Wiki-Protocol]]
- [[Agent-Interface]]
- [[Feature-Set]]
