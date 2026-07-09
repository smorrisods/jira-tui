# jira-tui distribution strategy

This document captures the agreed release shape for `jira-tui`'s first tagged releases and should evolve as the release automation and packaging work matures.

## Goals

- Ship `jira-tui` as a Linux- and macOS-first terminal application.
- Follow the broader Liminal HQ release shape: tagged GitHub Releases, generated release notes, attached artefacts, and checksums.
- Support `amd64` and `arm64` on both Linux and macOS.
- Keep releases operationally simple enough to rehearse and repeat.

## Release scope

Releases target:

- Linux `amd64`
- Linux `arm64`
- macOS `amd64` (cross-compiled `x86_64-apple-darwin`)
- macOS `arm64` (native `aarch64-apple-darwin`, Apple Silicon)

Each release publishes:

- a standalone `jira-tui` binary per platform/architecture
- a compressed release archive (`.tar.gz`, unpacking into `bin/` + `share/man/man1/`) per platform/architecture
- Linux-only package artefacts:
  - `.deb`
  - `.rpm`
- **one `SHA256SUMS` file covering every attached artefact** (not a `.sha256` per file — a single file is simpler to verify everything at once: `sha256sum -c SHA256SUMS`)
- a generated man page (`jira-tui.1`), gzipped, in both the Linux packages and every platform's tarball
- generated GitHub release notes, categorised via `.github/release.yml`
- `scripts/install.sh`: a curl-pipeable installer that detects OS/arch, downloads the matching archive, verifies it against `SHA256SUMS`, and installs the binary + man page (see "Install script" below)

Releases do not (yet) target:

- Windows
- crates.io publication
- the `jira-mcp` binary (the MCP server is a real, tested feature, but shipping it as a release artefact is deferred to keep releases rehearsable — revisit once the current flow has been running a while)

## Why this shape fits jira-tui

jira-tui behaves like a product binary rather than a library:

- the primary deliverable is the `jira-tui` TUI binary
- the binary now generates a man page at build time (see "Man pages" below)
- the README already describes jira-tui as a standalone terminal tool

That makes a GitHub Releases-first approach the cleanest fit.

## macOS: ad-hoc signed, not notarized

macOS builds are cross-compiled/built on a single `macos-14` (Apple
Silicon) runner — `aarch64-apple-darwin` natively, `x86_64-apple-darwin`
cross-compiled from the same runner, so no separate Intel runner is
needed. Binaries are **ad-hoc signed** (`codesign --sign - --force
--deep`) before archiving:

- This gives the binary a consistent local code identity, but it is
  **not** the same as Apple notarization — there's no Apple Developer
  account behind this yet, so Gatekeeper will still flag the binary as
  unidentified on first run.
- Documented plainly (README, install script output) rather than
  glossed over: users need to right-click → Open in Finder once, or run
  `xattr -d com.apple.quarantine jira-tui`.
- Full notarization (paid Developer account, `xcrun notarytool` in CI,
  stored credentials) is a real cost/complexity jump — revisit only if
  this grows beyond a small-team tool.
- No `.pkg` installer, no Homebrew tap yet — same reasoning as Linux's
  "no separate install script" *used to be* (see below, now reversed)
  and macOS's own installer/signing cost: keep the surface small until
  there's a concrete need. A Homebrew tap is the natural next step once
  there's a stable release cadence worth tracking that way.

## Install script

`scripts/install.sh` is a POSIX `sh` script (works whether it's run
directly or piped from `curl`) that:

- detects OS (`linux`/`darwin`) and architecture (`amd64`/`arm64`)
- resolves the latest release tag via the GitHub API, or installs a
  specific `--version`
- downloads the matching `.tar.gz` and `SHA256SUMS`, verifies the
  checksum, and refuses to install on a mismatch
- installs the binary + man page into `--prefix`/`$PREFIX` (default
  `/usr/local`), using `sudo` only if the prefix isn't writable
- supports `--uninstall` to remove a previous install
- prints coloured output (skipped for non-tty output or when `NO_COLOR`
  is set) and a small bit of Jax personality in the banner/completion
  message, matching the TUI's own tone

This *reverses* the original "no separate install script" stance from
this doc's first draft — worth calling out explicitly. The original
reasoning (extra trust surface, more to test and maintain) is real, but
weaker than it first appeared: the script just automates the same
curl → verify → extract → install steps we already tell people to run
by hand, over the same GitHub Releases HTTPS connection they'd already
need to trust to download a binary in the first place. For a
small-team tool, the UX win (one command, no manual arch/prefix
juggling) outweighs that marginal surface increase. Tested locally
end-to-end (install, checksum-mismatch rejection, and uninstall) against
a local HTTP server serving real built artefacts before shipping.

## Man pages: generated, not hand-authored

jira-tui doesn't use `clap`'s subcommand model the way some other Liminal HQ CLIs do, but it does use `clap::Parser` for its (flat) flag set. The man page is generated at build time from that same `Cli` definition (`src/cli.rs`) via `clap_mangen`, in `build.rs`:

- `src/cli.rs` is the **single source of truth** for the CLI surface — both `main.rs` (`Cli::parse()`) and `build.rs` (`include!("src/cli.rs")`, then `clap_mangen::Man::new(Cli::command())`) use the exact same struct.
- This avoids a footgun found while reviewing `flow`'s own release setup: `flow` defines its `Cli`/`Command` clap structs **twice** — once in `build.rs`, once in `main.rs` — so the two can silently drift apart over time. Filed as [liminal-hq/flow#51](https://github.com/liminal-hq/flow/issues/51). jira-tui's shared-`include!` approach makes that drift structurally impossible instead of just avoided by discipline.
- The generated page lands at `$OUT_DIR/man/jira-tui.1` during every build; packaging scripts discover it via the newest `*/build/*/out/man` directory next to the release binary (the same pattern `flow` uses to *discover* its generated man pages, even though its *generation* has the drift bug above).

## Proposed release flow

1. Merge the release-ready PR into `main`.
2. Run `scripts/prepare-release-version.sh --version <next-version>` in a clean working tree to create a release-bump branch and update `Cargo.toml`/`Cargo.lock` before tagging.
3. Open a PR from that branch, confirm CI is green, merge to `main`.
4. Create a tag such as `v0.2.0` on `main`.
5. GitHub Actions builds Linux and macOS artefacts for all supported architectures.
6. The workflow creates or updates the GitHub Release for that tag.
7. Binaries, packages, tarballs, and one `SHA256SUMS` file are attached.
8. Release notes are generated from merged PRs (via `.github/release.yml` category labels); do a quick install smoke test (`scripts/install.sh`, or manual extraction) from the uploaded assets before considering the release final.

Manual dispatch is also available (`workflow_dispatch`) so a tag can be rebuilt, or a draft release
prepared before publication. `release_tag` is a **required** input (e.g. `v0.1.0`) — it is never
derived automatically, so a manual run always targets an explicit, deliberate tag. The workflow also
refuses to attach new assets to a matching release that's already published (not a draft); it only
ever reuses a release that is still in its own draft state, so a manual dispatch can never silently
overwrite a real published release's assets.

## Version prep

Before tagging a release, use:

```bash
scripts/prepare-release-version.sh --version 0.2.0
```

By default, this creates and switches to a branch named `chore/release-v0.2.0` before updating files. Override with:

```bash
scripts/prepare-release-version.sh --version 0.2.0 --branch chore/my-custom-release-branch
```

The script updates:

- `Cargo.toml`
- `Cargo.lock` (only jira-tui's own `[[package]]` stanza, not any same-numbered dependency version)

It requires a clean working tree and prepares a branch meant to be reviewed and merged before the tag is created on `main`.

## GitHub Actions shape

`.github/workflows/release.yml` has four jobs:

- `prepare-release`
  - resolves the release tag (from the pushed tag, or from `workflow_dispatch` input / the package version)
  - creates or reuses a GitHub Release (idempotent reruns)
  - enables generated release notes
  - writes a concise summary to `$GITHUB_STEP_SUMMARY`
- `build-linux`
  - matrix over `amd64`/`arm64`, builds the release binary, the `.tar.gz` archive, and the `.deb`/`.rpm` packages
  - restores and saves Cargo build caches
  - uploads everything as workflow artefacts (per-architecture)
  - writes a concise summary to `$GITHUB_STEP_SUMMARY`
- `build-macos`
  - matrix over `amd64`/`arm64`, both built on a single `macos-14` runner (native `aarch64-apple-darwin`, cross-compiled `x86_64-apple-darwin`)
  - ad-hoc code-signs the binary, builds the `.tar.gz` archive (no `.deb`/`.rpm` equivalent on macOS)
  - uploads everything as workflow artefacts (per-architecture)
  - writes a concise summary to `$GITHUB_STEP_SUMMARY`
- `publish-release`
  - downloads every Linux and macOS artefact
  - generates **one `SHA256SUMS`** covering all of them
  - uploads everything (including `SHA256SUMS`) to the GitHub Release
  - writes a concise summary to `$GITHUB_STEP_SUMMARY`

Recommended matrix:

| runner | target triple | target label | architecture |
|---|---|---|---|
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `linux-amd64` | `amd64` |
| `ubuntu-22.04-arm` | `aarch64-unknown-linux-gnu` | `linux-arm64` | `arm64` |
| `macos-14` | `aarch64-apple-darwin` | `macos-arm64` | `arm64` |
| `macos-14` (cross-compiled) | `x86_64-apple-darwin` | `macos-amd64` | `amd64` |

Fixed runner images (not `-latest` aliases) keep the release environment predictable over time. Building Linux on Ubuntu 22.04 keeps the packaged `jira-tui` binary compatible with Ubuntu 22.04 and common WSL2 installs that still provide glibc 2.35. Building both macOS architectures from the same `macos-14` runner avoids needing a separate (and increasingly scarce) Intel Mac runner.

## Artefacts

Release asset naming:

- `jira-tui-<tag>-linux-amd64`
- `jira-tui-<tag>-linux-arm64`
- `jira-tui-<tag>-linux-amd64.tar.gz`
- `jira-tui-<tag>-linux-arm64.tar.gz`
- `jira-tui-<tag>-linux-amd64.deb`
- `jira-tui-<tag>-linux-arm64.deb`
- `jira-tui-<tag>-linux-amd64.rpm`
- `jira-tui-<tag>-linux-arm64.rpm`
- `jira-tui-<tag>-macos-amd64`
- `jira-tui-<tag>-macos-arm64`
- `jira-tui-<tag>-macos-amd64.tar.gz`
- `jira-tui-<tag>-macos-arm64.tar.gz`
- `SHA256SUMS` (one file, covering every artefact above)

Each archive includes:

- the `jira-tui` executable, at `bin/jira-tui`
- the generated man page, at `share/man/man1/jira-tui.1.gz`

## Packaging expectations

Linux packaging installs files into conventional package-managed locations:

- binary: `/usr/bin/jira-tui`
- man page: `/usr/share/man/man1/jira-tui.1.gz`

Manual archive installs (Linux or macOS) can use `/usr/local` or another
user-managed prefix — this is exactly what `scripts/install.sh` does by
default.

## Linux package metadata

- package name: `jira-tui`
- version: matches the Git tag without the leading `v`
- licence: `MIT`
- vendor: `Liminal HQ`
- maintainer: `Liminal HQ <contact@liminalhq.ca>`
- homepage: `https://github.com/smorrisods/jira-tui`
- architecture mapping:
  - Debian: `amd64`, `arm64`
  - RPM: `x86_64`, `aarch64`
- summary: `A developer-first, keyboard-driven Jira terminal UI`

Dependency stance:

- GNU-linked builds; Debian packages declare `Depends: libc6`
- RPM packages lean on `rpmbuild` auto-detection for shared library requirements
- musl / fully static builds are deferred until there's a concrete need

## Documentation updates required before release

Before the first tagged release, update `README.md` with:

- Linux and macOS installation instructions from release artefacts (done — see the "Installing a release build" section)
- package and archive install paths
- uninstall guidance

## Suggested `v0.2.0` (first tagged release) checklist

- [ ] Confirm `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, and `cargo nextest run` all pass across `default`, `--no-default-features`, and `--all-features`
- [ ] Run `scripts/prepare-release-version.sh --version 0.2.0`, review the diff, open and merge the version-bump PR
- [ ] Create tag `v0.2.0` on `main`
- [ ] Let the `Release` workflow build and publish
- [ ] Download the published artefacts and smoke-test on both Linux architectures (or via emulation) — extract the tarball, verify `SHA256SUMS`, install the `.deb`/`.rpm` in a container, run `jira-tui --demo`, and `man jira-tui`
- [ ] Smoke-test both macOS architectures (or on real hardware if available) — extract, ad-hoc signature present (`codesign -dv jira-tui`), confirm Gatekeeper behaves as documented, run `jira-tui --demo`
- [ ] Run `scripts/install.sh` against the real published release (not just the local HTTP-server test done during development) on at least one Linux and one macOS machine
- [ ] Confirm generated release notes look reasonable; edit if needed
- [ ] Update README with install/uninstall instructions if anything changed
