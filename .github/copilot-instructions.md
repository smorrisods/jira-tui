# Copilot instructions for jira-tui

`jira-tui` is a keyboard-driven Jira terminal UI in Rust (`ratatui` + `crossterm`), with an optional live REST client and an always-available offline demo mode.

## Architecture

- `src/domain` — stable internal models (`IssueSummary`, `IssueDetail`, `Source`) and the baked-in demo data.
- `src/adf` — Atlassian Document Format (ADF): render to styled terminal text, plus `to_markdown` / `compile` for the round-trip edit. Display + conversion.
- `src/jira` — the `live`-feature REST client (`ureq`): reads, workflow transitions, description writes, comments, and issue creation.
- `src/git` — repo/branch detection and `DS-123` issue-key parsing.
- `src/config` — XDG config/cache paths, settings, secure token file, onboarding marker, and the issue cache.
- `src/infra` — clipboard support via OSC 52, and opening URLs in the system browser.
- `src/mcp` — the `mcp`-feature Model Context Protocol server: exposes Jira read/write tools to agents, converting Markdown ⇄ ADF via `src/adf` so agents never construct raw ADF JSON. Reuses `src/jira::Config` for auth (same env vars / token file / `config.toml` as the TUI) and falls back to demo data for read tools when no credentials are configured. Served over stdio by the thin `src/bin/jira_mcp.rs` binary.
- `src/app` — application state, split by concern into submodules (`sort_filter`, `quick_view`, `search`, `board`, `transitions`, `edit`, `onboarding`, `mouse`, `detail`), with shared struct/constructor/tests (`tests.rs`) in `mod.rs`. Owns data loading, onboarding, round-trip edit, sort/filter, quick-view + list focus, search/go-to-issue, and the swimlane board (grouped by epic).
- `src/ui` — `ratatui` rendering, split by screen into submodules (`welcome`, `home`, `list`, `detail`, `search`, `board`, `preview`, `transition_picker`, `editor`, `jax_companion`, `about`, `help`), with the `draw()` dispatcher, theme, and shared chrome/helpers in `mod.rs`.
- `src/lib.rs` — library surface so integration tests can drive the real code; `src/main.rs` is a thin binary (CLI args, terminal lifecycle, async run loop) built on a `tokio` multi-thread runtime, with event handling in `src/keys.rs` and `$EDITOR` suspend/resume in `src/editor_launch.rs` (both binary-only modules). The run loop races a `crossterm::EventStream` against the animation tick via `tokio::select!`; the render/refresh/switch-view/detail-load/transition/edit/field-mapping/onboarding-verification call sites all dispatch live Jira REST calls onto `tokio::task::spawn_blocking` via `src/app/async_ops.rs` rather than blocking the render thread — demo/cache-only sessions still resolve inline since there's no network round-trip worth a spinner for. The async migration is complete top-to-bottom; there are no remaining synchronous live-network call sites.

## What to keep true

- **ADF-first:** render ADF structurally; never treat raw Markdown as stored content and never write Markdown strings into Jira. This applies equally to the MCP server: its tools accept/return Markdown but always convert through `adf::compile`/`adf::to_markdown` before touching Jira.
- **Demo mode never breaks:** no credentials and no network must still yield a fully explorable UI. Live mode is additive and falls back to cache, then demo.
- **Secrets:** the API token lives in a `0600` `token` file under the XDG config dir (or env / `token.txt`), never in `config.toml`.
- **Mouse mode is opt-in:** Shift-drag must fall through to native selection.
- **Preview before any mutating Jira call.**
- **Canadian spelling** in comments, docs, and UI copy (e.g. "colour"), except for external API fields and crate names.

## Build and test

- Build: `cargo build`. Offline-only build: `cargo build --no-default-features`.
- Test: `cargo test` (unit + `tests/cli.rs` + `tests/render.rs`). Keep `cargo clippy` clean under both feature sets and run `cargo fmt`.

## Writing style

- Don't hard-wrap prose at a fixed column in commits, PRs, issues, or Markdown docs — write each paragraph/bullet as one line and let it run long. See `AGENTS.md`'s "Writing style" section for the full rationale and exceptions (fixed-width content like ASCII wireframes).

## Commits

- Conventional Commits (`type(scope): summary`).
- Markdown bodies **without headings** — use **bold** labels for sections (**Summary**, **Why**, **Details**, **Validation**, **Risks**).
- Use `git commit -F <file>` so formatting and backticks survive.

See `AGENTS.md` for the full guidelines.

