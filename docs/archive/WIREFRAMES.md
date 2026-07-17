# jira-tui Low-Fidelity Wireframes

> **Historical:** describes the first implementation pass, since superseded
> by the shipped UI (and by the later UI-refresh mockups in
> `docs/archive/design/`). Kept for historical reference only.

These wireframes describe rough terminal layouts for the first implementation pass. They focus on information hierarchy and interaction flow rather than styling details.

## 1. Home Dashboard

```text
+----------------------------------------------------------------------------------+
| jira-tui                                                            DS / Scott   |
+----------------------------------------------------------------------------------+
| Context                                                                          |
| Repo: design-system                    Branch: DS-123-add-login                  |
| Inferred issue: DS-123                 Confidence: high                           |
+--------------------------------------+-------------------------------------------+
| Assigned To Me                        | Current Branch Issue                     |
|--------------------------------------|-------------------------------------------|
| DS-123  Add login flow               | DS-123                                   |
| DS-141  Fix date input copy          | Add login flow                           |
| DS-188  Review alert guidance        | Status: In Progress                      |
|                                      | Assignee: Scott                          |
|                                      | Next: Add comment / Transition / Link    |
+--------------------------------------+-------------------------------------------+
| Blocked                              | Quick Actions                            |
|--------------------------------------|-------------------------------------------|
| DS-144  API retry support            | [s] Start work                           |
| DS-172  Token refresh bug            | [t] Transition                           |
|                                      | [c] Comment                              |
|                                      | [n] New issue from branch                |
+----------------------------------------------------------------------------------+
| Recent: DS-123  DS-141  DS-188  DS-144                              [. ] Palette |
+----------------------------------------------------------------------------------+
```

## 2. Search View

```text
+----------------------------------------------------------------------------------+
| Search                                                                 / query    |
+----------------------------------------------------------------------------------+
| Mode: [ Issue Key ] [ JQL ] [ Recent ] [ Saved ]                                 |
+------------------------------+---------------------------------------------------+
| Results                      | Preview                                           |
|-----------------------------|---------------------------------------------------|
| DS-123  Add login flow      | DS-123                                            |
| DS-141  Fix date input copy | Story | In Progress | Scott                       |
| DS-188  Review guidance     |                                                   |
| DS-144  Retry support       | Summary                                           |
|                             | Add login flow for service account onboarding     |
|                             |                                                   |
|                             | Next actions                                      |
|                             | Assign | Transition | Comment | Open              |
+------------------------------+---------------------------------------------------+
| Enter: open   /: search   Tab: switch pane   Esc: back                           |
+----------------------------------------------------------------------------------+
```

## 3. Issue Detail View

```text
+----------------------------------------------------------------------------------+
| DS-123  Add login flow                                              In Progress  |
+----------------------------------------------------------------------------------+
| Identity                  | Narrative                                             |
|--------------------------|--------------------------------------------------------|
| Type: Story              | Description                                            |
| Priority: High           | Add login flow for service account onboarding...       |
| Assignee: Scott          |                                                        |
| Epic: Auth Modernisation | Acceptance Criteria                                    |
| Sprint: Sprint 42        | - user can start login                                |
|                          | - failures show useful state                           |
+--------------------------+--------------------------------------------------------+
| Links                     | Comments                                              |
|--------------------------|--------------------------------------------------------|
| blocks: DS-144           | Scott: investigating token edge case                  |
| relates: DS-188          | Priya: copy update is ready for review                |
+--------------------------+--------------------------------------------------------+
| Attachments                                                                       |
| design-notes.pdf     244 KB     [d] download   [o] open link   [x] delete        |
+----------------------------------------------------------------------------------+
| [e] edit   [t] transition   [c] comment   [a] assign to me   [l] link   [. ] cmd |
+----------------------------------------------------------------------------------+
```

## 4. Create Issue From Branch

```text
+----------------------------------------------------------------------------------+
| Create Issue From Branch                                                          |
+----------------------------------------------------------------------------------+
| Source context                                                                    |
| Repo: design-system         Branch: feature/date-input-copy                       |
| Suggested project: DS       Suggested summary: Fix date input copy                |
+----------------------------------------------------------------------------------+
| Type:        [ Story v ]                                                          |
| Summary:     [ Fix date input copy                                 ]              |
| Parent/Epic: [ Auth Modernisation                                 ]              |
| Labels:      [ forms, content                                      ]              |
| Assignee:    [ @me                                                 ]              |
+----------------------------------------------------------------------------------+
| Description preview                                                               |
| Create issue from current branch context and seed the initial summary.            |
+----------------------------------------------------------------------------------+
| [Enter] Create issue    [Ctrl-P] Preview payload    [Esc] Cancel                  |
+----------------------------------------------------------------------------------+
```

## 5. Command Palette

```text
+----------------------------------------------------------------------------------+
| > trans                                                                           |
+----------------------------------------------------------------------------------+
| Transition issue                                                                  |
| Transition current issue to In Progress                                           |
| Transition current issue to Done                                                  |
| View transitions for current issue                                                |
|                                                                                    |
| Recent                                                                            |
| Add comment                                                                       |
| Create issue from branch                                                          |
+----------------------------------------------------------------------------------+
```

## 6. Attachment Upload Flow

```text
+----------------------------------------------------------------------------------+
| Upload Attachment                                                                 |
+----------------------------------------------------------------------------------+
| Issue: DS-123  Add login flow                                                     |
| Path:  /home/scott/source/design-notes.pdf                                        |
| Size:  244 KB                                                                     |
|                                                                                    |
| Progress                                                                          |
| [##########################----------------------] 56%                            |
|                                                                                    |
| Status: Uploading to Jira...                                                      |
+----------------------------------------------------------------------------------+
| [Esc] Cancel upload                                                               |
+----------------------------------------------------------------------------------+
```

## 7. Narrow Terminal Fallback

```text
+--------------------------------------+
| DS-123  Add login flow               |
+--------------------------------------+
| In Progress | Scott | Story          |
| Epic: Auth Modernisation             |
+--------------------------------------+
| Description                          |
| Add login flow for service account   |
| onboarding...                        |
+--------------------------------------+
| Comments                             |
| Scott: investigating token case      |
+--------------------------------------+
| Actions                              |
| [t] transition                       |
| [c] comment                          |
| [d] download attachment              |
| [.] palette                          |
+--------------------------------------+
```
