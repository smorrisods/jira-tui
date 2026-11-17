# Agent Review: OpenCode (Claude Sonnet 5.5)

**Session date**: 2026-11-17
**Task**: Replace the `github-script` CI summary comment and `EnricoMi/publish-unit-test-result-action` in `jira-tui` with Liminal HQ Labs' `pr-results`, and contribute what `jira-tui` needed back to `pr-results`
**Agent**: OpenCode, Claude Sonnet 5.5

---

## What was built

In `jira-tui`: nextest JUnit uploaded per feature set, a `workflow_run` publisher pinned to the `pr-results` v0.3.0 commit, and a `.github/pr-results.toml` with one suite per feature set, each tied to its job, in the enhanced style with the commit's other checks listed. The old reporter job and the workflow-level `checks: write` and `pull-requests: write` permissions are gone.

In `pr-results`, three pull requests came out of it: `comment.other_checks` (the commit's other jobs in the comment), a guided style (now the default) with an enhanced mode for Liminal HQ projects, and the CI fixes those needed. `pr-results` v0.3.0 shipped them, and `jira-tui` was then moved onto that release and the enhanced style.

## What went well

- **Reading the tool's own spec first.** `SPEC.md` already listed the missing feature as S6, reserved as a settings error, so the contribution was finishing a planned item and not inventing one.
- **Mutation checks on the safety tests.** Removing a filter (or loosening the command allowlist) and confirming exactly the intended test fails caught tests that passed too easily on the first run.
- **Keeping the old output byte-identical.** Leaving the renderer's own default plain meant every existing golden stayed untouched, which made the review of the new goldens the only review.
- **Reading the existing CI before changing a default.** The smoke job's failure was predictable from the default change and was fixed by asserting the new default, not by loosening the check.

## What caused friction

- **The tool's name.** It was asked for as `pr-report` under `liminal-hq`; it is `pr-results` under `liminal-hq-labs`. Searching the named organisation found nothing, and the match was only found by trying the Labs organisation. When a name finds nothing, check sibling organisations before concluding it does not exist.
- **Two identities.** The `gh` account on the machine used for the first part had read-only access to the target repository, and commits there used a different email and were not signed the way the project's are. Pushing needed a fork and then a move to the machine with the normal account. Check `gh api repos/<repo> --jq .permissions` and the commit signing config before starting work that ends in a push.
- **A shell kill that matched itself.** `pkill -f "cargo nextest"` also matched the shell command that ran it and ended that command early, so a refactor that was meant to apply did not. Match processes by something the command line itself does not contain, or do not kill by pattern at all.
- **Quoting a multi-line patch through a shell here-document.** Rust string escapes inside Python inside a shell quote failed more than once. Writing the patch script to a file with the file tool, using raw strings, fixed it.
- **A newer local toolchain than the project pins.** Clippy on the machine's default toolchain flagged pre-existing code; the project's pinned toolchain did not. Run lints on the pinned version.
- **A changed default broke a check that tests defaults.** The action smoke job asserted plain output. A search for assertions of the old output before changing a default would have found it before CI did.

## Notes for next time

- `workflow_run` publishers only run from the default branch, so a pull request that adds one cannot show a comment; say so before anyone expects it.
- A reusable workflow pinned by commit SHA must be dereferenced from an annotated tag to the commit: `gh api repos/<repo>/git/ref/tags/<tag>` returns the tag object first.
- The first look at how GitHub renders a new comment style is the project's own pull request after it merges, not the golden files.
