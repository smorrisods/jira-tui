# Copilot instructions for jira-tui

`jira-tui` is a keyboard-driven Jira terminal UI in Rust (`ratatui` + `crossterm`),
with an optional live REST client and an always-available offline demo mode.

## Architecture

- `src/domain` — stable internal models (`IssueSummary`, `IssueDetail`, `Source`)
  and the baked-in demo data.
- `src/adf` — Atlassian Document Format (ADF): render to styled terminal text,
  plus `to_markdown` / `compile` for the round-trip edit. Display + conversion.
- `src/jira` — the `live`-feature REST client (`ureq`): reads, workflow
  transitions, and description writes.
- `src/git` — repo/branch detection and `DS-123` issue-key parsing.
- `src/config` — XDG config/cache paths, settings, secure token file, onboarding
  marker, and the issue cache.
- `src/infra` — clipboard support via OSC 52.
- `src/app` — application state, data loading, onboarding, transitions,
  round-trip edit, sort/filter, quick-view + list focus, search/go-to-issue,
  and the swimlane board (grouped by epic).
- `src/ui` — `ratatui` screens, theme, the welcome screen (Jax), the animated
  About panel, and the ambient Jax companion.
- `src/lib.rs` — library surface so integration tests can drive the real code;
  `src/main.rs` is a thin binary (terminal lifecycle + event loop).

## What to keep true

- **ADF-first:** render ADF structurally; never treat raw Markdown as stored
  content and never write Markdown strings into Jira.
- **Demo mode never breaks:** no credentials and no network must still yield a
  fully explorable UI. Live mode is additive and falls back to cache, then demo.
- **Secrets:** the API token lives in a `0600` `token` file under the XDG config
  dir (or env / `token.txt`), never in `config.toml`.
- **Mouse mode is opt-in:** Shift-drag must fall through to native selection.
- **Preview before any mutating Jira call.**
- **Canadian spelling** in comments, docs, and UI copy (e.g. "colour"), except
  for external API fields and crate names.

## Build and test

- Build: `cargo build`. Offline-only build: `cargo build --no-default-features`.
- Test: `cargo test` (unit + `tests/cli.rs` + `tests/render.rs`). Keep
  `cargo clippy` clean under both feature sets and run `cargo fmt`.

## Commits

- Conventional Commits (`type(scope): summary`).
- Markdown bodies **without headings** — use **bold** labels for sections
  (**Summary**, **Why**, **Details**, **Validation**, **Risks**).
- Use `git commit -F <file>` so formatting and backticks survive.

See `AGENTS.md` for the full guidelines.
