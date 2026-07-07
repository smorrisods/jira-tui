# Agent Review: GitHub Copilot CLI (Claude Opus 4.8)

**Session date**: 2026-07-07
**Task**: Bootstrap `jira-tui` — a Rust `ratatui` Jira terminal UI — from the
2026-03 design spec, with an offline demo mode, an ADF renderer, and an animated
about panel
**Agent**: GitHub Copilot CLI powered by Claude Opus 4.8

---

## What was built

A working Milestone-1 TUI: home dashboard (current context + at-a-glance stats +
my-work list), an ADF-rendered issue detail view, a full-list screen, a help
overlay, and a colour-wave animated ASCII about panel. Live Jira is wired through
a `ureq` client behind the `live` feature and falls back to baked-in demo data
whenever credentials or the network are missing, so the UI is always explorable.

## What went well

- **The demo-first decision paid off.** Building rich sample data with real ADF
  descriptions meant the whole UI — including the ADF renderer — was verifiable
  with zero credentials and zero network, via a PTY smoke test.
- **Porting the ADF renderer from `_render_issue.py` was clean.** The Python
  reference made the node/mark handling a straight translation.
- **Feature-gating networking behind `live`** kept a fast, warning-free offline
  build and a small dependency surface.
- **The self code-review caught real issues** (see below) that the compiler and
  clippy did not.

## What caused friction

- **No toolchain in the environment.** `rustc`/`cargo` and a C linker were both
  missing; needed a rustup install plus a `build-essential` install (which
  required the human, since `apt` wanted root). Worth front-loading a toolchain
  check before promising a Rust build.
- **PTY testing needs an explicit window size.** The first smoke test captured
  almost nothing because the pseudo-terminal defaulted to 0x0 and ratatui
  rendered into an empty rect. Setting `TIOCSWINSZ` fixed it — a good trick to
  record for any future TUI smoke tests here.
- **`cargo fmt` reshaped lines** between an edit being planned and applied, so a
  couple of string-match edits missed and had to be re-anchored.

## Code-review findings (all fixed this session)

| Finding | Severity | Fix |
|---|---|---|
| `fetch_my_work` used the sunset `/rest/api/3/search` endpoint | Medium | Switched to `/rest/api/3/search/jql` |
| `blocked` hardcoded false for live issues (dead ⛔ flag + stat) | Low | Request `issuelinks`, derive from inward "is blocked by" |
| Panic in the draw loop left the terminal raw + in alt-screen | Low | Installed a restoring panic hook |
| Dead-code warnings in the `--no-default-features` build | Low | Feature-gated `Config`; annotated live-only variants |

## Suggestions for the next session

- Milestone 2: quick transitions and assign-to-me, each with a preview/confirm
  step (keep the "preview before mutate" rule).
- Milestone 2/3: the Markdown round-trip edit — open an issue as a draft in
  `$EDITOR`, compile to ADF with the `jira-ds-skill` tooling, preview, then PUT.
- Move blocking HTTP off the render thread (a worker thread + channel) so live
  fetches don't freeze the UI.
- Add golden-ADF fixtures generated from the Python compiler as Rust snapshot
  tests, to guarantee the two ADF implementations stay in parity.

— GitHub Copilot CLI (Claude Opus 4.8), 2026-07-07
