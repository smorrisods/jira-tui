# jira-tui distribution strategy

This document captures the agreed release shape for `jira-tui`'s first
tagged releases and should evolve as the release automation and packaging
work matures.

## Goals

- Ship `jira-tui` as a Linux-first terminal application.
- Follow the broader Liminal HQ release shape: tagged GitHub Releases,
  generated release notes, attached artefacts, and checksums.
- Start with `amd64` and `arm64` Linux support only.
- Keep the first releases operationally simple enough to rehearse and
  repeat.

## Release scope

The first releases target:

- Linux `amd64`
- Linux `arm64`

Each release publishes:

- a standalone `jira-tui` binary
- a compressed release archive (`.tar.gz`, unpacking into `bin/` +
  `share/man/man1/`)
- Linux package artefacts:
  - `.deb`
  - `.rpm`
- **one `SHA256SUMS` file covering every attached artefact** (not a
  `.sha256` per file — a single file is simpler to verify everything at
  once: `sha256sum -c SHA256SUMS`)
- a generated man page (`jira-tui.1`), gzipped, in both the packages and
  the tarball
- generated GitHub release notes, categorised via `.github/release.yml`

The first releases do not target:

- macOS or Windows (see "macOS, later" below)
- crates.io publication
- a separate install script
- the `jira-mcp` binary (the MCP server is a real, tested feature, but
  shipping it as a release artefact is deferred to keep the first release
  rehearsable — revisit once the Linux flow is proven)

## Why this shape fits jira-tui

jira-tui behaves like a product binary rather than a library:

- the primary deliverable is the `jira-tui` TUI binary
- the binary now generates a man page at build time (see "Man pages" below)
- the README already describes jira-tui as a standalone terminal tool

That makes a GitHub Releases-first approach the cleanest fit.

## Man pages: generated, not hand-authored

jira-tui doesn't use `clap`'s subcommand model the way some other Liminal
HQ CLIs do, but it does use `clap::Parser` for its (flat) flag set. The man
page is generated at build time from that same `Cli` definition
(`src/cli.rs`) via `clap_mangen`, in `build.rs`:

- `src/cli.rs` is the **single source of truth** for the CLI surface —
  both `main.rs` (`Cli::parse()`) and `build.rs` (`include!("src/cli.rs")`,
  then `clap_mangen::Man::new(Cli::command())`) use the exact same struct.
- This avoids a footgun found while reviewing `flow`'s own release setup:
  `flow` defines its `Cli`/`Command` clap structs **twice** — once in
  `build.rs`, once in `main.rs` — so the two can silently drift apart over
  time. Filed as
  [liminal-hq/flow#51](https://github.com/liminal-hq/flow/issues/51).
  jira-tui's shared-`include!` approach makes that drift structurally
  impossible instead of just avoided by discipline.
- The generated page lands at `$OUT_DIR/man/jira-tui.1` during every
  build; packaging scripts discover it via the newest `*/build/*/out/man`
  directory next to the release binary (the same pattern `flow` uses to
  *discover* its generated man pages, even though its *generation* has the
  drift bug above).

## Proposed release flow

1. Merge the release-ready PR into `main`.
2. Run `scripts/prepare-release-version.sh --version <next-version>` in a
   clean working tree to create a release-bump branch and update
   `Cargo.toml`/`Cargo.lock` before tagging.
3. Open a PR from that branch, confirm CI is green, merge to `main`.
4. Create a tag such as `v0.2.0` on `main`.
5. GitHub Actions builds Linux artefacts for both supported architectures.
6. The workflow creates or updates the GitHub Release for that tag.
7. Binaries, packages, the tarball, and one `SHA256SUMS` file are
   attached.
8. Release notes are generated from merged PRs (via `.github/release.yml`
   category labels); do a quick install smoke test from the uploaded
   assets before considering the release final.

Manual dispatch is also available (`workflow_dispatch`) so a tag can be
rebuilt, or a draft release prepared before publication. When the manual
`release_tag` input is left blank, the workflow derives `v<package
version>` from `Cargo.toml` and validates the resolved tag before
continuing.

## Version prep

Before tagging a release, use:

```bash
scripts/prepare-release-version.sh --version 0.2.0
```

By default, this creates and switches to a branch named
`chore/release-v0.2.0` before updating files. Override with:

```bash
scripts/prepare-release-version.sh --version 0.2.0 --branch chore/my-custom-release-branch
```

The script updates:

- `Cargo.toml`
- `Cargo.lock` (only jira-tui's own `[[package]]` stanza, not any
  same-numbered dependency version)

It requires a clean working tree and prepares a branch meant to be
reviewed and merged before the tag is created on `main`.

## GitHub Actions shape

`.github/workflows/release.yml` has three jobs:

- `prepare-release`
  - resolves the release tag (from the pushed tag, or from
    `workflow_dispatch` input / the package version)
  - creates or reuses a GitHub Release (idempotent reruns)
  - enables generated release notes
  - writes a concise summary to `$GITHUB_STEP_SUMMARY`
- `build-linux`
  - matrix over `amd64`/`arm64`, builds the release binary, the `.tar.gz`
    archive, and the `.deb`/`.rpm` packages
  - restores and saves Cargo build caches
  - uploads everything as workflow artefacts (per-architecture)
  - writes a concise summary to `$GITHUB_STEP_SUMMARY`
- `publish-release`
  - downloads every Linux artefact
  - generates **one `SHA256SUMS`** covering all of them
  - uploads everything (including `SHA256SUMS`) to the GitHub Release
  - writes a concise summary to `$GITHUB_STEP_SUMMARY`

Recommended matrix:

| runner | target triple | target label | architecture |
|---|---|---|---|
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `linux-amd64` | `amd64` |
| `ubuntu-22.04-arm` | `aarch64-unknown-linux-gnu` | `linux-arm64` | `arm64` |

Fixed runner images (not `-latest` aliases) keep the release environment
predictable over time. Building on Ubuntu 22.04 keeps the packaged
`jira-tui` binary compatible with Ubuntu 22.04 and common WSL2 installs
that still provide glibc 2.35.

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
- `SHA256SUMS` (one file, covering every artefact above)

Each archive includes:

- the `jira-tui` executable, at `bin/jira-tui`
- the generated man page, at `share/man/man1/jira-tui.1.gz`

## Packaging expectations

Linux packaging installs files into conventional package-managed
locations:

- binary: `/usr/bin/jira-tui`
- man page: `/usr/share/man/man1/jira-tui.1.gz`

Manual archive installs can continue to use `/usr/local` or another
user-managed prefix.

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
- RPM packages lean on `rpmbuild` auto-detection for shared library
  requirements
- musl / fully static builds are deferred until there's a concrete need

## macOS, later (brainstorm — not yet planned in detail)

Not part of the current release scope, but sketched here so it's easy to
pick up when a teammate wants to run jira-tui on a Mac:

- **No code-signing/notarization budget for v1.** Without an Apple
  Developer account, a signed+notarized `.app`/`.pkg` isn't realistic yet.
  Gatekeeper will flag an unsigned/ad-hoc-signed binary; users would need
  to right-click → Open the first time, or run
  `xattr -d com.apple.quarantine jira-tui` after download. Document this
  clearly rather than pretending it's a polished experience.
- **Likely shape:** a `.tar.gz` per architecture (`macos-amd64`,
  `macos-arm64`), built on `macos-14` (Apple Silicon) and `macos-13` (or a
  Rosetta/x86_64 target cross-compiled from Apple Silicon) — same `bin/` +
  `share/man/man1/` layout as Linux, added to the same `SHA256SUMS` file.
- **Ad-hoc signing** (`codesign --sign -`) could at least satisfy local
  Gatekeeper checks without a paid developer account, worth testing before
  committing to the approach.
- **Homebrew tap** (`liminal-hq/homebrew-tap` or similar) is a natural
  follow-up once there's a stable release cadence — nicer install/update
  UX than a raw tarball, but it's more infrastructure (a formula to
  maintain, a tap repo) so it's explicitly a "later" item, not part of
  the first macOS pass.
- **No installer/.pkg** for the same reason as Linux's "no install
  script" — keep the trust surface and maintenance burden small until
  there's a reason to grow it.

## Documentation updates required before release

Before the first tagged release, update `README.md` with:

- Linux installation instructions from release artefacts
- package and archive install paths
- uninstall guidance

## Suggested `v0.2.0` (first tagged release) checklist

- [ ] Confirm `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
      and `cargo nextest run` all pass across `default`,
      `--no-default-features`, and `--all-features`
- [ ] Run `scripts/prepare-release-version.sh --version 0.2.0`, review the
      diff, open and merge the version-bump PR
- [ ] Create tag `v0.2.0` on `main`
- [ ] Let the `Release` workflow build and publish
- [ ] Download the published artefacts and smoke-test on both
      architectures (or via emulation) — extract the tarball, verify
      `SHA256SUMS`, install the `.deb`/`.rpm` in a container, run
      `jira-tui --demo`, and `man jira-tui`
- [ ] Confirm generated release notes look reasonable; edit if needed
- [ ] Update README with install/uninstall instructions

## Why not an install script

A separate install script could detect architecture, download the
matching release artefact, verify the checksum, unpack, and copy files
into a prefix. That's convenient, but it also adds maintenance overhead,
more surface area to test, and another trust path for users. For now,
package artefacts and documented manual archive extraction are the
simpler and more reliable release story.
