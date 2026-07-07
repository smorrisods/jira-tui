# Copilot instructions for jira-tui

`jira-tui` is a keyboard-driven Jira terminal UI in Rust (`ratatui` + `crossterm`),
with an optional live REST client and an always-available offline demo mode.

## Architecture

- `src/domain` — stable internal models (`IssueSummary`, `IssueDetail`, `Source`)
  and the baked-in demo data.
- `src/adf` — Atlassian Document Format (ADF) JSON rendered to styled terminal
  text. Display only.
- `src/jira` — configuration plus the `live`-feature REST client (`ureq`).
- `src/git` — repo/branch detection and `DS-123` issue-key parsing.
- `src/app` — application state, data loading, event-driven updates.
- `src/ui` — `ratatui` screens, theme, and the animated About panel.

## What to keep true

- **ADF-first:** render ADF structurally; never treat raw Markdown as stored
  content and never write Markdown strings into Jira.
- **Demo mode never breaks:** no credentials and no network must still yield a
  fully explorable UI. Live mode is additive and falls back gracefully.
- **Preview before any mutating Jira call.**
- **Canadian spelling** in comments, docs, and UI copy (e.g. "colour"), except
  for external API fields and crate names.

## Build and test

- Build: `cargo build`. Offline-only build: `cargo build --no-default-features`.
- Test: `cargo test`. Keep `cargo clippy` clean and run `cargo fmt`.

## Commits

- Conventional Commits (`type(scope): summary`).
- Markdown bodies **without headings** — use **bold** labels for sections
  (**Summary**, **Why**, **Details**, **Validation**, **Risks**).
- Use `git commit -F <file>` so formatting and backticks survive.

See `AGENTS.md` for the full guidelines.
