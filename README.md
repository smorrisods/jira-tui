# jira-tui 🍁

A developer-first, keyboard-driven **Jira terminal UI** written in Rust
(`ratatui` + `crossterm`) — fast, legible, ADF-native, and with a little bit of
soul (yes, there's an animated about panel).

It's the third act of a small trilogy:

1. **`jira-tasks`** — the proof-of-concept that showed LLMs can automate Jira
   through a code-reviewable Markdown → ADF pipeline.
2. **`jira-ds-skill`** — that pipeline packaged as a versioned agent skill.
3. **`jira-tui`** — this: a human-facing, at-a-glance way to browse and (soon)
   update your work without opening the browser.

## Why

Jira is powerful but heavy to operate from a terminal: too much field recall, and
most tools mirror the REST API instead of a developer's workflow. `jira-tui`
leads with intent — *what am I working on, what's blocked, what's next* — and
renders Jira's rich text (ADF) the way it's actually stored, so nothing leaks as
raw Markdown.

## Highlights

- **Always explorable.** Runs against built-in sample data with zero setup. Point
  it at real Jira when you're ready.
- **ADF-native rendering.** Headings, task lists, code blocks, tables, and inline
  marks render as structured terminal text — not flattened Markdown.
- **Git-aware.** Detects your repo and branch and elevates the `DS-123` issue in
  your current branch name.
- **At a glance.** Home dashboard with current context, assigned work, and
  blocked counts. Detail view with description, acceptance criteria, links, and a
  quick-transitions strip.
- **A bit of soul.** A colour-wave animated ASCII about panel (`a` or `--about`).

## Quick start

```bash
cargo run              # live if credentials exist, else demo
cargo run -- --demo    # force the offline sample data
cargo run -- --about   # open straight to the animated about panel
```

Build a release binary:

```bash
cargo build --release   # ./target/release/jira-tui
```

Offline-only build (no HTTP stack):

```bash
cargo build --no-default-features
```

## Live mode

Set credentials to load your real assigned work:

```bash
export JIRA_EMAIL="you@example.com"
export JIRA_API_TOKEN="…"          # or place it in a token.txt file
export JIRA_BASE_URL="https://your-org.atlassian.net"   # optional
export JIRA_PROJECT="DS"                                  # optional
```

Non-secret settings can also live in `~/.config/jira-tui/config.toml`:

```toml
base_url = "https://your-org.atlassian.net"
email = "you@example.com"
project = "DS"
```

Missing or invalid credentials never crash the app — it falls back to demo data.

## Keys

| Key | Action |
| --- | --- |
| `↑ / k`, `↓ / j` | move selection |
| `⏎ / l` | open the selected issue |
| `esc / h` | back (or quit from home) |
| `g` | go home |
| `l` | full list |
| `a` | about panel |
| `r` | refresh from source |
| `?` | toggle help |
| `q` | back / quit |

## Layout

```
src/
  domain/   stable models + demo data
  adf/      ADF JSON -> styled terminal text
  jira/     config + live REST client (feature: live)
  git/      repo/branch detection + key parsing
  app/      state, data loading, event updates
  ui/       ratatui screens, theme, animated about panel
docs/       product + technical design specs (SPEC, IMPLEMENTATION, …)
```

## Status

Milestone 1 (browse) is working end to end against demo and live data. Quick
transitions and the Markdown round-trip edit flow (compile a draft to ADF and
push via the `jira-ds-skill` tooling) are the next milestones — see `docs/` for
the full spec and roadmap.

## Guidelines

See `AGENTS.md`. In short: ADF-first, demo mode never breaks, preview before
mutate, Canadian spelling, and Conventional Commits with Markdown bodies (bold
section labels, no headings).
