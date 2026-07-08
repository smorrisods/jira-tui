# AGENTS

Guidelines for humans and AI agents working in `jira-tui`.

## Project

`jira-tui` is a developer-first, keyboard-driven Jira terminal UI written in
Rust (`ratatui` + `crossterm`). It should feel fast, legible, and have a little
personality — it does not need to be strictly utilitarian.

## Core Principles

- **ADF-first display.** Jira rich text is ADF (a JSON document tree). Render it
  as structured text (headings, task lists, code blocks) via `src/adf`. The
  round-trip edit converts ADF → Markdown for `$EDITOR` and recompiles Markdown →
  ADF; never write raw Markdown strings into Jira fields.
- **Demo mode always works.** The TUI must be fully explorable with zero network
  and zero credentials. Live Jira is an enhancement gated behind the `live`
  feature and the presence of credentials; missing creds fall back to the cached
  list, then to demo data, never a crash.
- **Onboarding is friendly.** First run shows a welcome screen that can collect
  and verify credentials or continue in demo. Secrets never land in
  `config.toml` — the API token is saved to a `0600` `token` file.
- **Preview before mutate.** Any action that changes Jira must be legible and
  confirmable first.
- **Intent over resources.** Lead with developer jobs (start work, open branch
  issue, triage blocked) rather than raw REST surface.
- **Respect XDG.** Config, token, onboarding marker, and cache live under the XDG
  config/cache directories.
- **Mouse is opt-in and polite.** Mouse mode is a toggle; Shift-drag must always
  fall through to the terminal's native selection.

## Rust Conventions

- Keep fast UI state separate from slow remote state.
- Domain models in `src/domain` stay stable even if Jira API shapes vary.
- Prefer small, readable functions; avoid clever borrows that hurt legibility.
- Run `cargo fmt` and keep `cargo clippy` clean (both the default and
  `--no-default-features` builds) before committing.
- Add tests for pure logic and rendering: unit tests live beside the code, and
  the integration suite in `tests/` covers the CLI (`tests/cli.rs`) and headless
  screen rendering via `ratatui`'s `TestBackend` (`tests/render.rs`). The library
  surface in `src/lib.rs` exists so tests can drive the real `app`/`ui`/`adf`.

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
- Use `test:` for test-only changes (including fixes to tests themselves) —
  reserve `fix:` for application-code bugs.

## Pull Requests

- **Branching:** work happens on a branch, not directly on `main`. Name
  feature branches `feature/<short-description>` and fix branches
  `fix/<short-description>` (add an issue number when one exists, e.g.
  `fix/issue-19-token-file-fallback`).
- **Titles:** human-readable summaries starting with a capital letter — no
  Conventional Commit prefixes (`feat:`, `fix:`, etc.) in the title itself.
  Describe the outcome/behaviour change, not internal process language.
- **Content:** PR descriptions must not mention internal workflow artefacts
  (session notes, todo-tracking mechanics, agent planning chatter) — keep
  that in `agent-reviews/` and local tooling, not in outward-facing PR text.
- **Description format:** compact Markdown with `## Summary` and
  `## Test plan`. Use `###` sub-sections under Summary when it helps group
  the change (e.g. `### User-facing changes`, `### Internals`,
  `### Documentation`). Flat bullets with bold lead-ins under each section.
  Under `## Test plan`, use checklist bullets (`- [x]`/`- [ ]`) naming the
  concrete commands run; state plainly anything that couldn't be verified.
- **Labels:** every PR gets at least one primary category label
  (`enhancement`, `bug`, `documentation`, `testing`, `ci`, `build`, `chore`)
  plus scope labels where useful (`rust`, `dependencies`, `github_actions`).
  Use `skip-changelog` for changes that shouldn't appear in release notes.
- **Readiness:** open PRs ready for review by default; only mark a PR draft
  when explicitly asked or when there's a clearly communicated blocker.
- **Force-pushing** an already-open PR branch (e.g. after a rebase) requires
  explicit user confirmation first — history rewrites on shared branches are
  disruptive.
- Prefer the `gh` CLI for PR/issue/label/workflow-run work over other GitHub
  tooling, unless it can't complete the task cleanly.

## Reviews

- Leave a dated note in `agent-reviews/` after a substantial session: what worked,
  what caused friction, and how it was handled. Read existing reviews first.
