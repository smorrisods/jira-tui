# AGENTS

Guidelines for humans and AI agents working in `jira-tui`.

## Project

`jira-tui` is a developer-first, keyboard-driven Jira terminal UI written in
Rust (`ratatui` + `crossterm`). It grew out of the `jira-tasks` proof-of-concept
and the `jira-ds-skill` ADF pipeline. It should feel fast, legible, and have a
little personality — it does not need to be strictly utilitarian.

## Core Principles

- **ADF-first display.** Jira rich text is ADF (a JSON document tree). Render it
  as structured text (headings, task lists, code blocks) via `src/adf`. Never
  show raw Markdown as if it were the stored content, and never write Markdown
  strings back into Jira fields.
- **Demo mode always works.** The TUI must be fully explorable with zero network
  and zero credentials. Live Jira is an enhancement gated behind the `live`
  feature and the presence of credentials; missing creds fall back to demo data,
  never a crash.
- **Preview before mutate.** Any action that changes Jira must be legible and
  confirmable first.
- **Intent over resources.** Lead with developer jobs (start work, open branch
  issue, triage blocked) rather than raw REST surface.

## Rust Conventions

- Keep fast UI state separate from slow remote state.
- Domain models in `src/domain` stay stable even if Jira API shapes vary.
- Prefer small, readable functions; avoid clever borrows that hurt legibility.
- Run `cargo fmt` and keep `cargo clippy` clean before committing.
- Add unit tests for pure logic (ADF rendering, branch parsing, mapping).

## Canadian English

- Use Canadian spelling in comments, docs, and user-facing copy where technically
  valid (e.g. "colour", "behaviour").
- Keep required external spellings unchanged for API fields, crate names, and
  third-party schema keys.

## Commits

- Use Conventional Commits: `type(scope): short summary`.
- Bodies are Markdown, but **do not use Markdown headings (`#`)** — use **bold**
  for section titles instead.
- Recommended body sections (as bold labels): **Summary**, **Why**, **Details**,
  **Validation**, **Risks**.
- Do not pass unescaped backticks to `git commit -m`. Prefer
  `git commit -F <message-file>` so backticks and formatting survive verbatim.
- Commit at meaningful milestones with a clear, detailed body.

## Reviews

- Leave a dated note in `agent-reviews/` after a substantial session: what worked,
  what caused friction, and how it was handled. Read existing reviews first.
