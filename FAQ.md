# Frequently Asked Questions

## What is FreeClawdia?
FreeClawdia (aka Clawdia) is a secure personal AI assistant that protects your data and expands its capabilities on the fly. It runs locally, keeps your data encrypted, and connects to multiple LLM providers.

## How do I start Clawdia?
Run `freeclawdia` from your terminal. The web gateway starts on the configured port (default 4000) and you can access it in your browser.

## What LLM providers are supported?
Clawdia supports NEAR AI, OpenAI, Anthropic (Claude), Ollama, any OpenAI-compatible API, and Tinfoil private inference. Set the provider via the `LLM_BACKEND` environment variable.

## How does the email integration work?
Clawdia connects to a JMAP-compatible mail server (like Stalwart) to read, send, search, move, and delete emails. The agent can interact with your email directly through natural language — ask it to summarize your inbox, draft replies, or search for specific messages. There's also a traditional email UI in the Email tab.

## What are Spaces?
Spaces let you group related chat threads together. Think of them as folders for your conversations — you might have a "Work" space, a "Personal" space, and a "Projects" space.

## Can the agent run tasks on a schedule?
Yes — Routines let you set up scheduled (cron) or event-driven tasks. The agent can check your email, run reports, or perform any other task on a recurring basis.

## What are Skills?
Skills are SKILL.md files that extend the agent's knowledge with domain-specific instructions. They activate automatically when your message matches their keywords. You can write your own or install them from the ClawHub registry.

## Is my data secure?
Yes. Secrets are encrypted with AES-256-GCM, stored locally, and never exposed to container processes. The agent has prompt injection defenses, content sanitization, and leak detection built in. Sandbox containers run with network proxies that control outbound access.

## How do I add new tools?
Tools can be built as WASM modules (sandboxed) or connected via MCP servers. Use `freeclawdia tool install` or the Extensions tab in the web UI.

## What is Time Travel?
The History tab lets you view the system's state at any point in the past. Set a date and time, click "Travel", and you'll see a reconstructed snapshot of settings, plus a timeline of every change that was recorded up to that moment. It's read-only — you're looking at the past, not changing it.

## What gets recorded in the History?
Changes to settings, threads (create/rename/delete), extensions (install/remove/activate), skills (install/remove), routines (create/update/delete/toggle), and secret management events. Email and external services are not tracked — the history reflects Clawdia's internal state only.
