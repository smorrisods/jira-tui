# jira-tui Interaction Spec

## Purpose

This document translates the product and wireframe docs into screen-level interaction behaviour for implementation.

## Global Interaction Model

The app should follow a consistent terminal interaction model:

- one active screen at a time
- one active focus region within that screen
- one selected item within list-like regions
- one global command palette available from almost anywhere

Every screen should expose:

- a primary action
- a secondary escape route
- a predictable focus order
- a small set of discoverable keyboard shortcuts

## Global Keys

These bindings should work across most screens unless a text input is focused.

| Key | Action |
|---|---|
| `.` | Open command palette |
| `/` | Focus search input or search mode |
| `Esc` | Close modal, clear transient UI, or go back one layer |
| `?` | Open help overlay |
| `g h` | Go to home |
| `g s` | Go to search |
| `g i` | Jump to issue by key |
| `Tab` | Move to next focus region |
| `Shift-Tab` | Move to previous focus region |
| `Enter` | Open, confirm, or activate selected control |
| `q` | Quit from top-level screens only |

## Focus Model

There are four major focus types:

- `List`: arrow or `j`/`k` movement through rows
- `Panel`: moves between larger screen regions
- `Input`: text editing and completion behaviour
- `Modal`: temporary exclusive interaction layer

Focus rules:

- only one region should be visibly focused at a time
- focused regions must have a stronger border or highlight
- selected rows must remain visible during refreshes when possible
- closing a modal should restore the last focused region beneath it

## Navigation Rules

### Lists

Use:

- `j` / `Down`: next row
- `k` / `Up`: previous row
- `Ctrl-d`: move down a larger chunk
- `Ctrl-u`: move up a larger chunk
- `gg`: jump to first row
- `G`: jump to last row

### Panels

Use:

- `Tab`: next region
- `Shift-Tab`: previous region
- `h` / `l`: optional left-right region movement where layout makes sense

### Inputs

Use standard text editing expectations:

- printable characters insert
- `Backspace` deletes left
- `Ctrl-w` deletes previous word
- `Ctrl-u` clears line where it does not conflict with paging
- `Enter` confirms input if the field is actionable

## Screen Specifications

## 1. Home Screen

### Purpose

Provide a fast overview of the user's current work context and the most likely next actions.

### Regions

1. top bar
2. context panel
3. assigned-to-me list
4. current-branch issue panel
5. blocked list
6. quick actions panel
7. footer or status line

### Default Focus

If a branch-linked issue is detected:

- focus `current-branch issue panel`

Otherwise:

- focus `assigned-to-me list`

### Primary Actions

- open selected issue
- start work on selected issue
- assign selected issue to self
- create issue from branch context

### Home Keys

| Key | Action |
|---|---|
| `Enter` | Open selected issue or activate quick action |
| `s` | Start work on focused issue |
| `a` | Assign focused issue to me |
| `c` | Add comment to focused issue |
| `t` | Open transition picker |
| `n` | Open create-from-context flow |
| `r` | Refresh dashboard data |

### Home Event Flow: Start Work

1. user focuses an issue card or row
2. user presses `s`
3. app checks valid transitions
4. if only one likely transition exists, show confirm modal
5. if many transitions exist, open transition picker
6. on success, update issue state in place and show toast

## 2. Search Screen

### Purpose

Help users find issues quickly by key, recent history, saved filter, or JQL.

### Regions

1. mode selector
2. query input
3. result list
4. preview pane
5. footer help line

### Search Modes

- issue key lookup
- fuzzy recent items
- saved filters
- JQL mode

### Default Focus

- query input on first entry
- result list after results populate

### Search Keys

| Key | Action |
|---|---|
| `Enter` | Open selected issue |
| `Ctrl-j` | Execute search in JQL mode |
| `Tab` | Move between query, results, and preview |
| `1` | Switch to issue key mode |
| `2` | Switch to recent mode |
| `3` | Switch to saved filters |
| `4` | Switch to JQL mode |
| `y` | Copy selected issue key |

### Search Event Flow: Open Issue

1. user types query
2. local matches appear immediately where possible
3. remote fetch begins for JQL or unresolved keys
4. user selects a result
5. preview pane updates
6. user presses `Enter`
7. issue detail screen opens with focus on identity or narrative region

## 3. Issue Detail Screen

### Purpose

Provide a dense but manageable view of one Jira issue and its adjacent actions.

### Regions

1. header
2. identity panel
3. narrative panel
4. links panel
5. comments panel
6. attachments panel
7. action bar

### Default Focus

- narrative panel if opened directly
- comments panel if opened from a comment action
- attachments panel if opened from an attachment action

### Issue Keys

| Key | Action |
|---|---|
| `e` | Toggle quick edit mode |
| `t` | Open transition picker |
| `c` | Open comment composer |
| `a` | Assign issue to me |
| `l` | Open link issue flow |
| `u` | Open upload attachment modal |
| `d` | Download selected attachment |
| `o` | Open selected attachment link or browser view |
| `x` | Delete selected attachment with confirmation |
| `J` | Jump focus to comments |
| `K` | Jump focus to attachments |
| `R` | Refresh issue data |

### Issue Event Flow: Quick Edit

1. user presses `e`
2. editable fields in the focused region become interactive
3. field-level validation appears inline
4. `Enter` saves the field
5. `Esc` cancels the edit and restores previous value
6. success shows a lightweight confirmation in the status area

### Issue Event Flow: Add Comment

1. user presses `c`
2. comment composer modal opens
3. user types markdown-like plain text
4. preview toggle shows rendered approximation if implemented
5. user confirms with `Ctrl-Enter`
6. app posts comment and appends it to the stream

## 4. Create Issue From Context Screen

### Purpose

Make issue creation fast by using repo and branch context to prefill the common fields.

### Regions

1. source context summary
2. issue type field
3. summary field
4. parent or epic field
5. labels field
6. assignee field
7. description preview
8. submit actions

### Default Focus

- summary field if a suggestion exists
- issue type field if no reliable project defaults exist

### Create Keys

| Key | Action |
|---|---|
| `Ctrl-p` | Preview payload |
| `Ctrl-Enter` | Submit create request |
| `Tab` | Move to next field |
| `Shift-Tab` | Move to previous field |
| `Ctrl-l` | Accept suggested labels |
| `Ctrl-e` | Open advanced fields drawer |

### Create Event Flow: Submit

1. user reviews or edits suggested fields
2. user presses `Ctrl-Enter`
3. app validates required fields locally
4. if Jira requires more fields, show advanced fields drawer with explicit guidance
5. on success, show created issue key and offer follow-up actions:
   - open issue
   - create branch
   - copy key
   - create another

## 5. Command Palette

### Purpose

Expose a fast action surface that works as the main expert entry point.

### Regions

1. query input
2. ranked result list
3. optional recent actions section
4. optional action hint footer

### Ranking Rules

Prefer, in order:

1. exact title match
2. actions relevant to current screen
3. actions relevant to selected issue
4. recently used actions
5. globally available actions

### Palette Keys

| Key | Action |
|---|---|
| `.` | Open palette |
| `Esc` | Close palette |
| `Enter` | Run selected action |
| `Ctrl-k` | Clear query |
| `Tab` | Toggle between all and contextual actions if implemented |

### Palette Event Flow: Contextual Action

1. user opens palette from issue detail
2. current issue becomes implicit action context
3. user types `trans`
4. transition-related actions rank first
5. user selects `Transition current issue to In Progress`
6. action runs immediately or asks for confirmation if needed

## 6. Attachment Upload Modal

### Purpose

Provide a clear, safe, and observable file upload experience.

### Regions

1. issue summary
2. path input
3. file metadata preview
4. progress area
5. status line
6. action buttons

### Default Focus

- path input

### Upload Keys

| Key | Action |
|---|---|
| `Enter` | Validate selected path |
| `Ctrl-Enter` | Start upload |
| `Esc` | Cancel modal or in-progress upload if supported |
| `Tab` | Move between controls |

### Upload Event Flow

1. user opens modal with `u`
2. user types or pastes a path
3. app validates file existence and size
4. file metadata preview appears
5. user confirms upload
6. progress bar updates while streaming
7. success refreshes attachment panel and shows toast

## 7. Transition Picker Modal

### Purpose

Make status changes fast without requiring workflow memorisation.

### Regions

1. issue summary
2. transition list
3. transition detail or field requirements
4. confirm area

### Default Focus

- transition list

### Transition Keys

| Key | Action |
|---|---|
| `Enter` | Select transition or confirm if already selected |
| `Space` | Preview transition details |
| `Esc` | Close modal |

### Transition Event Flow

1. user presses `t`
2. app fetches allowed transitions
3. list opens with likely next state preselected
4. selecting a transition reveals any required extra fields
5. confirmation submits transition request
6. issue detail updates in place

## 8. Help Overlay

### Purpose

Keep the tool discoverable without leaving the current workflow.

### Regions

1. global key list
2. screen-specific keys
3. navigation tips
4. close hint

### Help Keys

| Key | Action |
|---|---|
| `?` | Open or close help |
| `Esc` | Close help |
| `/` | Search help entries later if implemented |

## Feedback and Status Behaviour

Transient feedback types:

- success toast
- warning banner
- inline validation message
- persistent error banner when action failed
- loading indicator in panel or modal title

Status messages should be specific, for example:

- `Assigned DS-123 to you`
- `Downloaded design-notes.pdf to ~/Downloads/jira-tui`
- `Could not transition DS-123: Resolution is required`

## Empty States

Every list-style region should have an explicit empty state.

Examples:

- `No assigned issues right now`
- `No attachments on this issue`
- `No branch-linked Jira issue detected`
- `No transitions available from current status`

Each empty state should offer one next action when possible.

## Error States

Common UI-level error handling:

- auth expired: show re-auth action
- network failure: preserve visible data and offer retry
- validation failure: focus first failing field
- attachment failure: preserve modal and show exact cause
- unknown API response: show a safe summary and offer raw diagnostics

## Implementation Notes

- Keep screen actions declarative so they can be tested without full terminal rendering.
- Model focus changes explicitly in state rather than inferring them from UI shape.
- Keep modal flows isolated so they can be reused across screens.
- Treat success feedback as part of navigation confidence, not cosmetic polish.
