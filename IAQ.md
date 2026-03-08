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

## Future considerations
- Anthropic API direct access (currently blocked by account login issues, using NEAR AI)
- Gmail extension integration possibility
- Further email UI enhancements
