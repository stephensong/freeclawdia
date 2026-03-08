# Infrequently Asked Questions

Design decisions, architectural context, future plans, and development notes for FreeClawdia.

## Why "FreeClawdia"?
A rebrand of IronClaw (upstream: nearai/ironclaw) with minimal changes — just the binary name, base directory, and metadata. The crate stays `ironclaw` so all internal paths work unchanged and rebases from upstream are nearly conflict-free.

## Why a minimal rebrand?
To stay close to upstream and make syncing easy. Only 3 files differ: `Cargo.toml`, `src/bootstrap.rs`, `.gitignore`. This means upstream features and fixes can be rebased with minimal conflicts.

## Why JMAP for email instead of IMAP?
JMAP is a modern, JSON-based protocol that's much simpler to work with than IMAP. It supports push notifications, efficient sync, and structured queries. Stalwart mail server provides a solid JMAP implementation that runs locally.

## Why Stalwart?
Stalwart is a Rust-based mail server that supports JMAP, IMAP, and SMTP. It's lightweight, runs locally, and is easy to configure. It runs on port 4010 in our setup to avoid conflicts with other services.

## Port allocation strategy
- 4000: Gary's Clawdia instance
- 4001: Emma's Clawdia instance
- 4002: Iris's Clawdia instance
- 4010: Stalwart mail server
- Moved from 3000-series to avoid conflicts with freenet (3001) and other services.

## Email UI design philosophy
The email tab provides a traditional three-pane layout (sidebar, message list, reading pane) with a horizontal splitter. However, the real power is the agent's direct email tools — the UI is for manual browsing, but users should be encouraged to let the agent handle email tasks via natural language. Marketing term: "emAIl".

## FAQ and IAQ documents
- `FAQ.md` — User-facing, covers common questions about using Clawdia
- `IAQ.md` (this file) — Developer/maintainer-facing, captures design decisions, plans, and session context
- Both served at runtime via API (not compiled in) so edits don't require recompilation
- Presented in the web UI with an accordion interface
- IAQ is actively maintained by the AI assistant to preserve context across sessions

## Multi-user setup
Gary, Emma, and Iris each run their own Clawdia instance with separate `.env` files, ports, and user IDs. Each instance connects to the same Stalwart mail server. Browser tabs show "clawdia — gary" etc. via server-side HTML injection in `index_handler()`.

## Testing philosophy
E2E tests preferred over manual testing. Tests are cumulative and fast-fail — run in sequence, fix one failure at a time. Each test must also be independently runnable (idempotent).

## Current development focus
- Email integration refinements (UI polish, agent email tools)
- Web gateway UX improvements (context menus, splitters, floating windows)
- Context retention via FAQ/IAQ documents
- Time travel / temporal database (feature branch: `feature/time-travel`)

## Time travel — temporal database design
Inspired by Date & Darwen's temporal relational model and Datomic's immutable append-only architecture.

**Core principle:** Agents operate in "now" — all writes go to the current epoch, all reads default to current state. The temporal layer is purely retrospective and read-only. Agent code needs zero changes. Time travel is a query-time concern, not a write-time concern.

**Transaction time only** (not valid time). We care about "when was this fact recorded" not "when was this fact true in the world." This is the right model for an AI assistant's audit trail.

**Phased approach:**
- **Phase 1: Audit log** — New `audit_log` table capturing all mutations as JSON diffs. Pure INSERT, works on both PostgreSQL and libSQL. No changes to existing queries or agent code.
- **Phase 2: Point-in-time reconstruction** — Replay audit log to reconstruct any entity's state at time T. Read-only "View as at" UI. Covers conversations, messages, settings, thread metadata.
- **Phase 3: Native temporal tables** — PostgreSQL-only. System-versioned tables with `tstzrange` validity periods, GiST indexes, Date & Darwen's interval model. Only after Phase 2 is validated.

**Key constraints:**
- **Dual-backend:** Phase 1-2 work on both backends. Phase 3 is PostgreSQL-only (libSQL lacks range types).
- **External state:** Email (Stalwart), MCP servers, WASM tools are not in our DB — time travel shows our recorded state, not external world state. UI must communicate this clearly.
- **Storage:** Append-only means unbounded growth. Retention policies needed (e.g., compact >90 days).
- **Scope guard:** Initial temporal entities are conversations + messages + settings only. Resist the temptation to make everything temporal at once.

**UI concept:** "View system as at:" accepting a datetime, switching the entire view to historical read-only state.

## Future considerations
- Anthropic API direct access (currently blocked by account login issues, using NEAR AI)
- Gmail extension integration possibility
- Further email UI enhancements
