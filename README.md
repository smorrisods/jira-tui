# jira-tui 🍁

A developer-first, keyboard-driven **Jira terminal UI** written in Rust
(`ratatui` + `crossterm`) — fast, legible, ADF-native, mouse-friendly, and with a
little bit of soul (there's an animated about panel and a mascot named Jax).

It's the third act of a small trilogy:

1. **`jira-tasks`** — the proof-of-concept that showed LLMs can automate Jira
   through a code-reviewable Markdown → ADF pipeline.
2. **`jira-ds-skill`** — that pipeline packaged as a versioned agent skill.
3. **`jira-tui`** — this: a human-facing, at-a-glance way to browse and (soon)
   update your work without opening the browser.

## Highlights

- **Guided onboarding.** First launch greets you with a welcome screen (and Jax)
  that can collect and verify your Jira credentials, or drop you into demo mode.
- **Always explorable.** Runs against built-in sample data with zero setup, and
  caches your last live "my work" list for instant, offline starts.
- **ADF-native rendering.** Headings, task lists, code blocks, tables, and inline
  marks render as structured terminal text — not flattened Markdown.
- **Git-aware.** Detects your repo and branch and elevates the `DS-123` issue in
  your current branch name.
- **Mouse mode + clipboard.** Optional click-to-open, wheel scroll, and
  drag-to-copy via OSC 52 — with Shift-drag reserved for native terminal
  selection.
- **A bit of soul.** A colour-wave animated ASCII about panel (`a` or `--about`).

## Quick start

```bash
cargo run              # live if credentials exist, else demo (welcome on first run)
cargo run -- --demo    # force the offline sample data
cargo run -- --about   # open straight to the animated about panel
cargo run -- --onboard # re-run the welcome / live setup
cargo run -- --init    # scaffold ~/.config/jira-tui/config.toml, then exit
```

Build a release binary:

```bash
cargo build --release   # ./target/release/jira-tui
```

Offline-only build (no HTTP stack):

```bash
cargo build --no-default-features
```

## First run & onboarding

On first launch (when no config exists yet) jira-tui shows a welcome screen:

- **`s` — Set up live access:** enter your Jira **site**, **email**, and **API
  token**. The token is **masked**, **verified** against Jira (`/myself`), then
  saved to `~/.config/jira-tui/token` with `0600` permissions (never in
  `config.toml`). Non-secret settings are written to `config.toml`.
- **`d` — Continue in demo:** keep browsing the sample data.
- **`w` — Write config:** scaffold a default `config.toml` you can edit by hand.

Re-run it any time with `--onboard`. The non-interactive `--init` just writes the
default config file.

## Live mode

You can also configure credentials without the wizard:

```bash
export JIRA_EMAIL="you@example.com"
export JIRA_API_TOKEN="…"          # or ~/.config/jira-tui/token, or a token.txt file
export JIRA_BASE_URL="https://your-org.atlassian.net"   # optional
export JIRA_PROJECT="DS"                                  # optional
```

Non-secret settings live in `$XDG_CONFIG_HOME/jira-tui/config.toml`
(default `~/.config/jira-tui/config.toml`):

```toml
base_url = "https://your-org.atlassian.net"
email = "you@example.com"
project = "DS"
mouse = false   # start with mouse mode on/off
```

Missing or invalid credentials never crash the app — it falls back to the last
cached list, then to demo data.

## Editing

Inside an issue (`Detail`):

- **`t` — change status:** opens a transition picker; pick a target and it's
  applied (via Jira REST in live mode, locally in demo). The current status is
  marked, and a toast confirms the move.
- **`e` — edit the description:** serialises the issue's ADF to Markdown, opens
  it in **`$VISUAL`/`$EDITOR`** (falling back to `vi`), then **recompiles your
  Markdown back to ADF** and shows a **preview**. Press `y` to apply (REST in
  live mode) or `esc` to cancel — nothing is sent to Jira until you confirm.

The Markdown ↔ ADF conversion follows the same mapping rules as the
`jira-ds-skill` pipeline (headings, bullet/ordered/task lists, code blocks, and
inline `code`/**bold**/*italic*/links), so the round trip stays ADF-native.

## Mouse & clipboard

Press **`m`** to toggle mouse mode:

- **Click** a row to open that issue.
- **Wheel** to scroll the list or issue detail.
- **Drag** to select rows; on release the text is copied to your system clipboard
  via **OSC 52** (works over SSH, no X11/Wayland dependency).
- **Shift-drag** bypasses the app so your terminal's **native selection/copy**
  works as usual.

You can also yank without the mouse: **`y`** copies the selected issue key, **`Y`**
copies its browse URL.

## Keys

| Key | Action |
| --- | --- |
| `↑ / k`, `↓ / j` | move selection (scroll in detail) |
| `⏎ / l` | open the selected issue |
| `esc / ← / ⌫` | back (or quit from home) |
| `g` | go home |
| `l` | full list |
| `t` | change status (in an issue) |
| `e` | edit description in `$EDITOR` |
| `a` | about panel |
| `m` | toggle mouse mode |
| `y` / `Y` | copy issue key / URL to clipboard |
| `r` | refresh from source |
| `?` | toggle help |
| `q` | back / quit |

## Files & XDG paths

| Path | Purpose |
| --- | --- |
| `$XDG_CONFIG_HOME/jira-tui/config.toml` | non-secret settings |
| `$XDG_CONFIG_HOME/jira-tui/token` | API token, `0600` |
| `$XDG_CONFIG_HOME/jira-tui/.onboarded` | first-run marker |
| `$XDG_CACHE_HOME/jira-tui/my-work.json` | cached "my work" list |

## Layout

```
src/
  domain/   stable models + demo data
  adf/      ADF <-> styled text and ADF <-> Markdown (render, to_markdown, compile)
  jira/     live REST client: read + transitions + description writes (feature: live)
  git/      repo/branch detection + key parsing
  config/   XDG config, settings, token, and issue cache
  infra/    clipboard (OSC 52)
  app/      state, data loading, onboarding, transitions, round-trip edit
  ui/       ratatui screens, theme, welcome (Jax), animated about, toasts
  lib.rs    library surface (so tests can drive the real code)
  main.rs   thin binary: terminal lifecycle, event loop, $EDITOR launch
tests/      cli.rs (process) + render.rs (headless TestBackend)
docs/       product + technical design specs (SPEC, IMPLEMENTATION, …)
```

## Testing

```bash
cargo test        # unit + integration suite
cargo clippy --all-targets
cargo clippy --no-default-features --all-targets   # offline build stays clean
```

The suite covers ADF rendering (including malformed input), branch-key parsing,
config/cache/token lifecycle, selection and mouse logic, the credential form, the
CLI surface (`--version`, `--help`, `--init`), and headless rendering of every
screen via `ratatui`'s `TestBackend`.

## Status

Milestone 1 (browse) and Milestone 2 (quick transitions + the Markdown
round-trip edit) are working end to end against demo, cached, and live data, with
onboarding, mouse mode, and clipboard support. Richer edit flows and attachments
are next — see `docs/` for the full spec and roadmap.

## Guidelines

See `AGENTS.md`. In short: ADF-first, demo mode never breaks, preview before
mutate, Canadian spelling 🍁, and Conventional Commits with Markdown bodies (bold
section labels, no headings).
