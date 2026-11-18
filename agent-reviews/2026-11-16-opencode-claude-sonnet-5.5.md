# Agent Review: OpenCode (Claude Sonnet 5.5)

**Session date**: 2026-11-16
**Task**: Audit the repo before a release, then fix the open round-trip and link bugs as a `gh stack` of PRs (#170–#176), with each branch implemented by a subagent and reviewed by me
**Agent**: OpenCode powered by Claude Sonnet 5.5, with Claude Opus 5.5 for the plan and general-purpose subagents for the implementation

---

## What was built

Seven stacked PRs, bottom to top: issue-link direction (#167), the stale detail fetch after a locally created issue (#119), `localId` on checklists (#136), strikethrough (#166), nested lists (#165), tables (#164, #110), and Jira's error response body (#169, filed during the session).

## What went well

- **A read-only plan first paid off.** The Opus plan checked each issue against the current code and caught stale line numbers in #119, that #164 also closes #110, and that `to_markdown` was dropping nested task lists. It also set the stack order, which kept the four `compile.rs` branches from conflicting.
- **One subagent per branch, with a precise brief.** Each brief named the files, the design decisions already made, the tests wanted, and what not to do (no commits, no branch changes). Reviewing the diff and committing myself kept the history clean and the commit bodies in the repo's format.
- **The regression test for #119 was checked against the unfixed code.** It failed without the fix, which is worth asking for every time.
- **The sweeps for assumptions were cheap.** Before changing the error text in #169 the subagent grepped for anything matching on `status code`, and found nothing.

## What caused friction

- **Issue numbers.** The request said #138, which is an unrelated merged PR. It was cheap to check with `gh issue view 138`, and the user's correction settled it. Check a number against the title before planning around it.
- **`gh stack submit --auto`.** It created PRs with Conventional Commit titles, no labels, and generic bodies, which the PR rules in `AGENTS.md` forbid. I fixed all seven with `gh pr edit` afterwards. The interactive editor wasn't usable from an agent, so this is the cost of stacking from one.
- **A dropped connection mid-session.** One subagent call failed with `ECONNRESET`. Nothing was lost because each branch was already committed, so I checked `git status` and `gh stack view` and re-ran the call.
- **The Dependabot PR (#163)** still fails CI on a compile error and was left out of scope.

## Suggestions for the next session

- None of the fixes were verified against a live Jira instance. The most valuable manual checks are a `Blocks` link from the TUI and MCP (#167), and a checklist and a table edit through `update_description_markdown` (#136, #164).
- Links created since #156 may already be reversed in Jira and will need correcting by hand.
- Known gaps left alone: combined marks such as `**~~x~~**` don't compile, tables with merged cells flatten on a round trip, and the MCP tool still doesn't read a link back after creating it (#168 tracks delete).
