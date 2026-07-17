# jira-tui Specification

> **Historical:** the original pre-implementation spec. Superseded in practice
> by the shipped codebase (see `CLAUDE.md`/`AGENTS.md`) and by
> `docs/archive/design/SPEC.md` for the later UI-refresh phase. Kept for
> historical reference only.

## 1. Vision

Build a modern Rust TUI for Jira that feels as clean and task-oriented as `gh`, while respecting Jira's more complex data model.

The product should help developers move through work with minimal friction:

- find what matters now
- create issues from current context
- update work in a few keystrokes
- link code and Jira naturally
- inspect details without opening a browser unless they choose to

## 2. Problem Statement

Jira is powerful but often expensive to operate in a terminal because:

- core actions require too much field recall
- teams work in Git context, but Jira tools often ignore it
- enterprise custom fields leak into the common path
- attachment, linking, and transition flows are often incomplete or awkward
- many tools mirror the API rather than the developer's workflow

The result is a terminal experience that feels like resource management instead of work management.

## 3. Product Principles

### 3.1 Local Context First

If the user is in a Git repo, `jira-tui` should detect:

- repository name
- current branch
- likely issue key in branch name
- current remote host and owner
- recent commit messages

This context should shape defaults before asking the user to fill anything manually.

### 3.2 Intent Over Resources

The interface should lead with user jobs such as:

- pick up work
- update work
- create follow-up
- link work to code
- review blockers
- prepare handoff

It should not require users to think first in terms of raw REST resources.

### 3.3 Progressive Disclosure

The common path should show only the fields and actions needed most often. Advanced controls should be available on demand through expansion, drill-in, or a command palette.

### 3.4 Speed With Recovery

Fast keyboard paths should exist for common actions, but every action should also be legible, reversible where safe, and previewable before destructive changes.

### 3.5 Human Output By Default

The default TUI should optimise for scanning, not schema fidelity. Structured export and raw JSON views should remain available for debugging and trust-building.

## 4. Target Users

### Primary

- developers working from a terminal
- technical leads triaging and assigning work
- release coordinators managing issue state during delivery

### Secondary

- QA and support staff using a keyboard-first workflow
- project maintainers who want better visibility without living in the browser

## 5. Mental Model

`jira-tui` should present Jira through five top-level concepts:

1. Inbox
2. Work
3. Plan
4. Create
5. Inspect

### Inbox

What needs my attention now?

- assigned to me
- mentioned recently
- blocked items
- stale review items
- issues related to current branch or repo

### Work

What am I actively doing?

- current branch issue
- in-progress work
- recently viewed issues
- quick transitions
- comments and links

### Plan

What is next?

- sprint items
- backlog slices
- saved filters
- dependency chains

### Create

What new work should exist?

- create from branch
- create from commit range
- create sub-task
- create linked follow-up
- create bug from failure context

### Inspect

What is the full truth?

- all fields
- changelog
- raw JSON
- linked issues
- attachments
- workflow transitions

## 6. Core Experience

### 6.1 Startup Flow

On launch, the app should:

1. detect auth state
2. detect whether the user is inside a Git repo
3. infer active Jira context from branch naming conventions
4. load a home dashboard with actionable sections

If no Git repo is detected, the TUI should still work, but it should shift emphasis toward search, inbox, and saved filters.

### 6.2 Home Dashboard

The landing screen should act like a terminal-native control centre.

Suggested panels:

- `Current context`
- `Assigned to me`
- `Current branch issue`
- `Recently updated`
- `Blocked`
- `Quick actions`

Example quick actions:

- `Start work`
- `Transition`
- `Comment`
- `Create linked issue`
- `Open in browser`
- `Copy issue key`

### 6.3 Command Palette

A command palette should be the main escape hatch for expert users.

Examples:

- `Create issue from branch`
- `Transition issue`
- `Assign to me`
- `Add comment`
- `Link issue`
- `Attach file`
- `Open raw JSON`
- `View changelog`

Palette behaviour:

- fuzzy search
- recency bias
- contextual ranking
- keyboard-first selection

### 6.4 Issue Detail View

The issue detail screen should be split into digestible regions:

- identity: key, summary, type, status, priority
- ownership: assignee, reporter, sprint, epic
- flow: transitions, blockers, linked issues
- narrative: description, acceptance criteria, recent comments
- metadata: labels, components, custom fields
- diagnostics: raw JSON, field IDs, API trace on demand

The default issue view should stay compact. Expanded panels should reveal heavier metadata only when requested.

### 6.5 Quick Edit Flow

Editing should feel closer to `gh pr edit` than to a form builder.

Common edits should be available inline:

- summary
- status transition
- assignee
- labels
- sprint
- comment
- estimate

Heavier edits should open a side panel or modal with field-level validation.

## 7. Developer-First Features

### 7.1 Git-Aware Context

The TUI should parse issue keys from:

- branch names such as `DS-123-add-login`
- commit subjects
- PR titles where integration exists

When an issue key is inferred, the home screen should elevate that issue automatically.

### 7.2 Branch-Centred Workflow

The user should be able to:

- view the issue tied to the current branch
- start a branch from an issue
- rename a branch to include an issue key
- create a new issue and a matching branch together
- copy branch naming suggestions

### 7.3 Next Action Bias

Each screen should answer: what is the next likely useful action?

Examples:

- if unassigned and in view: show `Assign to me`
- if status permits progress: show `Start work`
- if blocked: show `Add blocker comment` or `Link blocker`
- if no branch exists: show `Create branch`
- if an issue lacks acceptance criteria: show `Add criteria`

### 7.4 Rich Linking

Linking work should be first-class:

- link to current branch
- link to commit SHA
- create related issue from current one
- link blockers and duplicates quickly
- show reverse links clearly

### 7.5 Attachment Experience

Unlike many early Jira CLIs, attachment handling should be complete.

Required actions:

- list attachments
- preview metadata
- upload file
- download file
- delete file with confirmation
- open attachment location or copied link

Uploads and downloads should show progress, size, and failure state clearly.

## 8. Information Architecture

Recommended high-level navigation:

```text
Home
Inbox
My Work
Search
Issue
Create
Filters
Settings
Help
```

## 9. Interaction Design

### 9.1 Layout

The interface should feel modern and spacious rather than dense and legacy-styled.

Layout guidance:

- two- or three-pane layouts on wide terminals
- single-column drill-down on narrow terminals
- generous spacing around key actions
- clear selection states
- muted metadata, stronger emphasis on status and summaries

### 9.2 Visual Language

Design for a modern terminal aesthetic:

- calm neutral base
- limited accent colours by meaning
- status colours with textual reinforcement
- subtle borders and panel depth
- restrained motion for loading and state changes

### 9.3 Keyboard Model

Suggested bindings:

- `j` / `k`: move list selection
- `Enter`: open or confirm
- `Tab`: move focus region
- `/`: search
- `.`: command palette
- `e`: edit current issue
- `c`: comment
- `t`: transition
- `a`: assign to me
- `l`: link issue
- `u`: upload attachment
- `d`: download attachment
- `g h`: go home
- `g i`: go to issue by key
- `?`: help

## 10. Functional Requirements

Must support:

- issue view
- issue search
- issue create
- issue edit
- transitions
- comments
- linking
- attachments
- watcher operations
- project and field metadata

## 11. Architecture

### 11.1 Language and Libraries

Rust stack recommendation:

- `ratatui` for rendering
- `crossterm` for terminal events
- `tokio` for async runtime
- `reqwest` for HTTP
- `serde` / `serde_json` for data models
- `tracing` and `tracing-subscriber` for diagnostics

### 11.2 Proposed Crate Structure

```text
jira-tui/
  Cargo.toml
  src/
    main.rs
    app/
    ui/
    domain/
    infra/
    git/
    jira/
    commands/
    config/
```

### 11.3 API Strategy

Do not mirror Jira endpoints one-to-one in the UI layer.

Instead:

- create domain-level operations such as `assign_to_me`
- use adapters to map these intents onto Jira REST calls
- keep raw endpoint access available in diagnostics and low-level modules only

This is the key architectural move that separates the TUI from a thin API wrapper.

## 12. v1 Scope

Include in v1:

- auth
- home dashboard
- search
- issue detail
- quick edit
- comments
- transitions
- create flow
- link flow
- attachment list, upload, download, delete
- Git context detection
- command palette

Exclude from v1:

- board visualisation
- sprint planning drag-and-drop
- Confluence integration
- plugin ecosystem

## 13. Roadmap

### Phase 1: Foundation

- auth and profile management
- API client and metadata loading
- Git context engine
- basic TUI shell and routing

### Phase 2: Daily Workflow

- dashboard
- search and issue detail
- quick edits and transitions
- comments and linking

### Phase 3: Creation and Attachments

- guided issue creation
- branch-aware creation flows
- attachment upload and download
- richer validation

## 14. Open Questions

- Which Jira Cloud auth flow is least painful in a terminal while remaining secure?
- How much Jira field metadata should be cached locally?
- Which branch naming conventions should ship as defaults?
- Should the TUI expose markdown editing and convert to ADF automatically?
- Do we need a companion CLI mode for scripting outside the TUI?

## 15. Guiding Summary

The best version of `jira-tui` should not feel like `Jira, but in a terminal`.

It should feel like:

- a developer cockpit
- a Git-aware work console
- a terminal-native Jira companion
