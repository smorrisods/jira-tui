# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`jira-tui` is a developer-first, keyboard-driven Jira terminal UI written in Rust (`ratatui` + `crossterm`), with an optional live REST client and an always-available offline demo mode.

This repo already has detailed human/agent guidelines in `AGENTS.md` (writing style, commit format, PR conventions, release process, Canadian spelling) — read it in full before making changes. `.github/copilot-instructions.md` is a condensed mirror of the same rules for Copilot. Follow `AGENTS.md` as the source of truth for conventions; this file focuses on commands and architecture.

## Commands

```bash
cargo build                       # default build (live feature: HTTP stack + sqlite cache)
cargo build --no-default-features # offline-only build (no HTTP stack)
cargo build --all-features        # includes the jira-mcp binary

cargo run                         # live if credentials exist, else demo
cargo run -- --demo               # force offline sample data
cargo run -- --init               # scaffold ~/.config/jira-tui/config.toml, then exit

cargo test                        # unit tests + tests/cli.rs + tests/render.rs + tests/mcp.rs
cargo test <name>                 # run a single test by name (substring match)
cargo test --no-default-features  # run tests under the offline feature set
cargo nextest run --workspace     # matches CI's runner (see .config/nextest.toml)

cargo fmt --all -- --check        # CI's format check
cargo clippy --workspace --all-targets -- -D warnings                       # default features
cargo clippy --workspace --all-targets --no-default-features -- -D warnings # offline
cargo clippy --workspace --all-targets --all-features -- -D warnings        # incl. mcp
```

CI (`.github/workflows/ci.yml`) runs format, clippy, and tests across three feature sets (`default`, `no-default-features`, `all-features`), plus `cargo-audit`. Match that matrix locally before pushing — a change that's clean under `default` can still fail under `--no-default-features` (no `ureq`/`rusqlite`) or `--all-features` (adds the `mcp` async server).

## Architecture

The codebase splits into `src/domain` (stable internal models, independent of Jira's API shapes) plus feature-oriented modules that consume it:

- `src/domain` — `IssueSummary`, `IssueDetail`, `Source`, and the baked-in demo data used when there's no network/credentials.
- `src/adf` — Atlassian Document Format: render ADF to styled terminal text, and `to_markdown`/`compile` for the round-trip edit (ADF → Markdown for `$EDITOR`/built-in editor, Markdown → ADF before writing back). Raw Markdown is never written directly into a Jira field.
- `src/jira` — the `live`-feature REST client (`ureq`): reads, workflow transitions, description writes, comments, issue creation.
- `src/git` — repo/branch detection and `DS-123`-style issue-key parsing from the current branch name.
- `src/config` — XDG config/cache paths, settings, the secure `0600` token file, onboarding marker, and the issue cache.
- `src/cache` — the on-disk SQLite cache backing the offline/last-known-good issue list (one entry per view: My Work, All Project Issues, each teammate).
- `src/infra` — clipboard via OSC 52 and opening URLs in the system browser.
- `src/mcp` — the `mcp`-feature Model Context Protocol server exposing Jira read/write tools to agents; converts Markdown ⇄ ADF via `src/adf` so agents never construct raw ADF JSON, reuses `src/jira::Config` for auth, and falls back to demo data when no credentials are configured. Served over stdio by `src/bin/jira_mcp.rs`.
- `src/app` — application state, split by concern (`sort_filter`, `quick_view`, `search`, `board`, `transitions`, `edit`, `onboarding`, `mouse`, `detail`, `assign`, `comments`, `links`, `history`, `tree`, `view_switch`, `async_ops`, `field_mapping`, `palette`), with the struct/constructor in `mod.rs`, the top-level data loader in `loader.rs`, small cross-cutting query helpers in `query.rs`, and unit tests in `tests/` (split by the screen/flow each test exercises, mirroring this same concern split). Owns data loading, onboarding, round-trip edit, sort/filter, quick-view + list focus, search/go-to-issue, the command palette, and the swimlane board (grouped by epic).
- `src/ui` — `ratatui` rendering, split by screen (`welcome`, `home`, `list`, `quick_view`, `detail`, `search`, `board`, `preview`, `transition_picker`, `assignee_picker`, `editor`, `field_mapping`, `view_picker`, `jax_companion`, `palette`, `about`, `help`), with the `draw()` dispatcher, theme, and shared chrome (`header`, `footer`, and `keymap`'s help-overlay hint registry) in `mod.rs`. Per-screen responsive breakpoints live in sibling `*_columns` modules (`list_columns`, `home_columns`, `quick_view_columns`, `board_columns`, `detail_columns`).
- `src/lib.rs` — library surface so integration tests can drive the real `app`/`ui`/`adf` code. `src/main.rs` is a thin binary (CLI args, terminal lifecycle) built on a `tokio` multi-thread runtime, with event handling in `src/keys/` (split into the main `handle_key` dispatch, the Welcome onboarding key map, and mouse input) and `$EDITOR` suspend/resume in `src/editor_launch.rs` (both binary-only). The run loop races a `crossterm::EventStream` against the animation tick via `tokio::select!`; live Jira REST calls (render/refresh/switch-view/detail-load/transition/edit/field-mapping/onboarding-verification) dispatch onto `tokio::task::spawn_blocking` via `src/app/async_ops/` rather than blocking the render thread — demo/cache-only sessions resolve inline since there's no network round-trip worth a spinner for.

### Feature flags

- `live` (default): pulls in `ureq` (HTTP) and `rusqlite` (bundled SQLite) for talking to real Jira and caching results. Disabling it (`--no-default-features`) yields a pure offline/demo build with no HTTP stack.
- `mcp`: adds the `jira-mcp` binary; implies `live`.

### Man page

The man page is generated at build time from `src/cli.rs` via `clap_mangen` (`build.rs`) — never hand-edit it. `src/cli.rs` is the single source of truth for the `Cli` struct, shared by `main.rs` and `build.rs`.

## What to keep true

- **ADF-first:** render ADF structurally; never treat raw Markdown as stored content, never write Markdown strings into Jira fields.
- **Demo mode never breaks:** no credentials and no network must still yield a fully explorable UI. Live mode is additive and falls back to cache, then demo, never a crash.
- **Secrets:** the API token lives in a `0600` `token` file under the XDG config dir (or env var / `token.txt`), never in `config.toml`.
- **Mouse mode is opt-in:** Shift-drag must fall through to the terminal's native selection.
- **Preview before any mutating Jira call.**
- **Canadian spelling** in comments, docs, and UI copy (e.g. "colour"), except for external API fields and crate names.
