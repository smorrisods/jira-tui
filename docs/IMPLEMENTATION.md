# jira-tui Implementation Notes

## Purpose

This document translates the product spec into a practical implementation plan for a Rust-based Jira TUI.

## Core Tools

The initial implementation should use these Rust libraries and platform tools:

- `rustc` and `cargo`: build, test, and package management
- `ratatui`: terminal UI rendering and layout
- `crossterm`: terminal input, alternate screen handling, keyboard events, and cross-platform terminal support
- `tokio`: async runtime for UI tasks, background fetches, and concurrent API work
- `reqwest`: HTTP client for Jira REST API calls and attachment transfers
- `serde` and `serde_json`: serialisation for Jira payloads, config, and cached data
- `anyhow`: application-level error propagation with context
- `thiserror`: typed domain and infrastructure errors
- `tracing` and `tracing-subscriber`: structured logging and diagnostics
- `directories`: OS-specific config, cache, and data directory discovery
- `keyring`: secure storage for Atlassian credentials or tokens where supported
- `rusqlite`: lightweight local cache for recent issues, filters, and metadata
- `fuzzy-matcher` or `nucleo-matcher`: fuzzy search for the command palette and issue lookup
- `arboard` or terminal-safe clipboard integration later if needed for copy actions
- Git CLI or `git2`: repository detection and branch metadata

## Recommended External Integrations

- Jira Cloud REST API for work item data, comments, transitions, attachments, and metadata
- Atlassian auth flow, likely API token first for MVP, with browser/device flow investigated later
- Local Git repository context from `.git` and current branch state

## Implementation Strategy

### 1. Keep UI Intent-Centric

The UI should call domain actions such as:

- `assign_to_me`
- `start_work`
- `create_issue_from_branch`
- `add_comment`
- `upload_attachment`

These actions should then map to Jira API calls through a dedicated adapter layer.

### 2. Separate Fast UI State From Slow Remote State

Keep two kinds of state:

- immediate UI state: focus, selection, open panels, command palette text
- remote data state: issues, transitions, comments, metadata, attachments

This separation keeps the interface responsive while network work continues in the background.

### 3. Prefer Readable Domain Models

Key domain entities:

- `IssueSummary`
- `IssueDetail`
- `TransitionOption`
- `AttachmentItem`
- `CommentItem`
- `ProjectProfile`
- `GitContext`
- `CommandAction`

These should be stable internal models even if Jira API shapes vary by endpoint.

## Proposed Project Layout

```text
jira-tui/
  Cargo.toml
  README.md
  SPEC.md
  IMPLEMENTATION.md
  WIREFRAMES.md
  src/
    main.rs
    app/
      mod.rs
      state.rs
      events.rs
      router.rs
      actions.rs
    ui/
      mod.rs
      theme.rs
      layout.rs
      widgets/
      screens/
    domain/
      mod.rs
      issue.rs
      comment.rs
      attachment.rs
      transition.rs
      project.rs
      git_context.rs
    jira/
      mod.rs
      client.rs
      auth.rs
      models.rs
      mapping.rs
      endpoints/
    git/
      mod.rs
      detect.rs
      branch.rs
      parse.rs
    infra/
      mod.rs
      cache.rs
      persistence.rs
      logging.rs
      tasks.rs
    commands/
      mod.rs
      palette.rs
      registry.rs
    config/
      mod.rs
      profile.rs
      keys.rs
      settings.rs
```

## Screen-by-Screen Implementation Notes

### Home

Needs:

- dashboard layout manager
- background fetch orchestration
- current Git context widget
- list widgets for assigned, blocked, and recent work
- quick action dispatcher

### Search

Needs:

- query input widget
- local fuzzy result list
- remote JQL execution mode
- pagination state
- saved query support

### Issue Detail

Needs:

- split pane layout
- collapsible sections
- inline edit affordances
- comments stream
- transitions menu
- attachment panel

### Create Flow

Needs:

- guided form state
- defaults from Git context
- issue type templates
- preview mode before submit
- validation and required-field handling

### Command Palette

Needs:

- action registry
- fuzzy matcher
- context-aware ranking
- recent action history

## Data Flow

Suggested runtime pattern:

1. terminal event enters app loop
2. router maps event to screen action
3. action updates immediate UI state
4. async task runs if remote work is required
5. task result updates domain state
6. UI re-renders from the latest state snapshot

## Caching Plan

Cache these items locally:

- recently viewed issues
- recent search results
- project metadata
- field metadata
- recent transitions
- current user profile

Do not cache attachment file contents by default.

## Attachment Implementation Notes

Attachment support should include:

- upload by local path
- streamed download to configured directory
- progress reporting
- duplicate filename handling
- permission and size error handling

Use `reqwest` streaming for both upload and download so the UI can show progress rather than freezing.

## Git Context Implementation Notes

Preferred order of detection:

1. discover repository root
2. read current branch name
3. parse issue key from branch using configured regex
4. inspect recent commit messages if branch parse fails
5. present inferred key with confidence level

## Configuration Files

Suggested local config path:

```text
~/.config/jira-tui/config.toml
```

Suggested settings:

- default site
- default project
- branch key regex
- download directory
- theme name
- key bindings
- favourite filters
- field visibility preferences

## MVP Milestones

### Milestone 1

- terminal shell
- auth setup
- Git context detection
- issue search
- issue detail view

### Milestone 2

- dashboard
- quick transitions
- comments
- assign to me
- create issue

### Milestone 3

- attachment upload and download
- command palette
- caching
- richer edit flows

## Testing Strategy

### Unit Tests

Focus on:

- branch parsing
- Jira payload mapping
- command ranking
- reducers and state transitions
- config parsing

### Integration Tests

Focus on:

- mocked Jira API flows
- auth handling
- attachment upload and download paths
- create and edit flows

### UI Snapshot Tests

Where practical, snapshot:

- home dashboard layout
- issue detail layout
- command palette results
- error states

## Risks

- Jira field variability may complicate generic editing flows
- auth UX may be awkward if Atlassian terminal-friendly flows are limited
- terminal rendering differences may affect layout polish across environments
- attachment uploads may need careful API handling and retry logic

## Recommended First Build Order

1. app shell and event loop
2. theme and base layout
3. Jira client and auth
4. Git context service
5. search and issue detail
6. quick actions
7. create flow
8. attachment workflows
