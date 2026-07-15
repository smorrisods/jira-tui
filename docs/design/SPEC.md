# UI refresh — implementation spec

This is the hand-off spec for implementing the UI refresh mocked up in `ui-refresh.html` (landscape) and `ui-refresh-portrait.html` (portrait). The mockups are the visual source of truth; this document is the behavioural source of truth. Read `AGENTS.md` and `CLAUDE.md` before starting — commit format, Canadian spelling, the ADF-first rule, and the CI feature matrix all apply.

## Scope and phasing

Everything here is presentation-layer plus a small number of new keybindings and app-state fields. No Jira REST changes are required except where explicitly noted (transitions from the list/board). Suggested phase order, each independently shippable:

1. Theme + chips + selection style (foundation for everything else)
2. Footer hint groups with the no-wrap drop rule
3. Header breadcrumb + sync pill
4. List columns, tree guides, view flipping
5. Detail two-column / facts-panel layouts
6. Board cards + narrow column pager
7. Quick view split, Home tiles/strips
8. Mini-Jax
9. Command palette (largest new surface, last)

## 1. Theme

Extend the constants in `src/ui/mod.rs`. Terminal palettes are indexed colours in practice; the mockup hex values are targets for truecolor terminals — fall back to the nearest ANSI colour when truecolor is unavailable (ratatui `Color::Rgb` vs named colours; keep the existing named-colour constants as the fallback set).

| Token | Hex | Fallback | Role |
|---|---|---|---|
| `ACCENT` (cyan) | `#62D8D3` | `Cyan` | brand, panel titles, issue keys, kbd hints |
| `ACCENT2` (orchid) | `#C79BF0` | `Magenta` | secondary panel titles, Jax, In Review |
| `MAPLE` (new) | `#E8834A` | `LightRed` | selection bar + tint, focus, Jax moments — nothing else |
| `OK` (fern) | `#8FCB7A` | `Green` | Done, success toast, sync LED |
| `WARN` (amber) | `#E3B564` | `Yellow` | Medium priority, labels, WIP markers, filter state |
| `DANGER` (ember) | `#E5716B` | `Red` | Highest/High context, blocked, "is blocked by" |
| `MUTED` | `#77838F` | `DarkGray` | secondary text |
| `FAINT` (new) | `#4A5763` | `DarkGray` | tertiary text, tree guides, group labels |

**Selection style (everywhere):** replace the magenta `▌` cursor glyph and the board's `Rgb(40,40,80)` background with: `MAPLE` left bar (a `▌` in maple, or `Block` left border on cards) + a low-alpha maple background tint on the row/card + bold summary. One selection language across list, board, pickers, and palette.

**Chips:** rework `chip()` from black-text-on-solid-colour to coloured-text-on-tinted-background (the colour at ~14% alpha over the panel background; precompute blended RGB values since terminals have no alpha). Status chips: To Do (muted), In Progress (cyan), In Review (orchid), Done (fern). Type chips: Bug (ember), Story (fern), Task (blue `#6FB3E0`), Epic (orchid). Label chips: amber.

## 2. Chrome

**Header (3 rows incl. borders, unchanged height):**
- Left: brand (`jira` cyan / `-tui` orchid) then a breadcrumb: `{view} › {screen}` with the current node bold — e.g. `My Work › List`, `All Project Issues › Board`, `List › DS-129 · ← 3 back` on Detail (the `← N back` count comes from the existing history stack). Active filter appears as an amber crumb node.
- Right: git context (`⎇ branch ⇢ DS-123`, existing) plus a sync pill: `● live · synced 2m ago` — LED fern when fresh, amber when serving cache, muted `● demo` in demo mode. Below ~90 cols the pill collapses to `● 2m`.

**Footer (3 rows incl. borders, unchanged height) — the no-wrap rule:**
- Hints are *groups*: a faint uppercase label (`NAV`, `VIEW`, `ACT`, `GO`) followed by `key description` pairs, keys rendered in the kbd style (cyan on panel2 with a bottom-heavy border look — in a TUI, cyan text is sufficient).
- **The footer never wraps to a second line.** Measure the rendered width; drop whole groups right-to-left until it fits. `? all keys` is always the last survivor. The right side keeps the spinner + status text and truncates before hints drop.
- Group content per screen is in the mockup footers. Note the mockups deliberately dropped some existing hints (e.g. `y/Y` copy on Detail) — those keys still work and live in help + palette.

## 3. List screens (Home right panel + List)

**Columns**, replacing the packed row string, in priority order: selection bar · priority glyph · key (with tree guide) · type chip · status chip · summary · assignee (initials avatar + name) · updated (relative). A one-line faint uppercase column header row separates from data. Column drop order as width shrinks: assignee → type → updated. Key, status, summary never drop. The header row only shows present columns.

**Narrow two-line selected row:** below ~90 cols, the *selected* row grows a second line carrying exactly the dropped columns (type chip, assignee, parent key). Unselected rows stay one line.

**Tree guides (parent ↔ child):** in tree mode (`T`), draw box-drawing guides in the key column, coloured `FAINT`: `▾ ` on a parent whose children are visible (`▸ ` when collapsed, if collapse is implemented), `├─ ` on each child except the last, `└─ ` on the last child, and `│` continuation in the glyph column of rows whose ancestor rail passes through. Guides live in the key column so they survive every column-drop breakpoint. Flat mode keeps no guides. Also applies to the tree rows already rendered in `src/app/tree.rs` output.

**View flipping:** the panel title becomes `◂ {view label} ▸`. New keys `<` and `>` (rendered `‹›` in hints) cycle View: My Work → All Project Issues → teammate 1 → … → wrap. Cycling reuses the exact same data path as `confirm_view_switch` (cache-then-live, spinner in footer). `V` keeps opening the picker for direct jumps. `<`/`>` are currently unbound — verified against `src/keys.rs`.

**Sort/filter state:** stays in the panel title as muted metadata (`· tree · priority ↓ · bugs · 4 of 6`), and the active filter also shows in the footer as `f bugs ✕` (pressing `f` continues to cycle; the ✕ communicates "there is state to clear").

## 4. Quick view

Split panel instead of full detail re-render. Wide (≥ ~100 cols): description excerpt left, compact meta grid right (type, status, priority, assignee, parent, labels, updated). Narrow: chips row → inline `key value` pairs (wrapping flex) → description excerpt. Overflow line: `… ↓ N more lines`. Tab-focus behaviour unchanged.

## 5. Home

Wide: left rail (~32 cols) with three cards — context (repo/branch/linked issue + status chip), at-a-glance (counts with 4-cell proportion bars: assigned=fern, blocked=ember, in-review=cyan, done-this-week=muted), recent (last 3 from history stack). Right: the shared list panel.

Narrow: rail becomes stacked strips — context as one wrapped line in a short panel; glance as a 4-up row of tiles (label / big number / 1-row bar); **recent collapses to a single muted line under the list, and is the first thing to disappear entirely when height is short** (explicitly agreed: hide recents when there's no space). The list keeps the remaining height.

## 6. Detail

Wide (≥ ~110 cols): two columns. Main (flexible width): identity line (key + summary bold + chips), description panel (ADF rendering unchanged — layout only), activity panel (comments as cards: 2-cell left rule — maple for own comments, edge-colour otherwise — author bold + faint timestamp, ADF body; `n/p`/`]`/`[` unchanged). Side rail (~34 cols): workflow panel (transition strip: each status in a dashed outline chip, current one solid orchid bold; `t` unchanged), people & meta panel (kv grid), links panel (relation word coloured: ember for blocks, fern for relates), children panel (key + type/status chips).

Narrow: single column, order: identity + chips → **facts panel** (two-pairs-per-row kv grid with the workflow strip inside; new key `x` folds it to one line — `x` is unbound, verified) → description → **linked panel** (links and children merged, relation prefix coloured: ember blocks / fern relates / orchid child) → activity. Acceptance criteria render as `☐`/`☑` checklist items when the mapped AC field parses as a list.

## 7. Board

Wide: cards instead of the text grid. Column headers: status-coloured name + faint count + optional amber `wip N`. Epic swimlane headers: `▾ {epic key} · {name} · N issues` in orchid. Cards: priority glyph + key + type chip / summary / assignee avatar + faint age; blocked cards get a `⛔` chip. Selected card: maple border + bar + tint. Empty cells: one dashed ghost line. Fully-done lanes collapse behind `pgdn`.

Narrow (< ~90 cols): **column pager.** A tab strip of all statuses (chips; current = cyan outline bold, others muted) with `←`/`→` arrows at the ends; only the current column's cards render, full width, still grouped by epic lane with `1 here · 3 total` counts. The selected card shows a neighbour peek line: `◂ To Do 2 · In Review 0 ▸` (counts of this epic's issues in adjacent columns). Lanes with nothing in the current column collapse to one ghost line (`▸ 2 lanes with nothing In Progress — pgdn to peek`). Keys unchanged: `↑↓` card, `←→` column (now pages), `pg↕` lane, `⏎` open.

**Proposed addition (new REST surface):** `t` on a selected card/list row opens the transition picker for that issue without entering Detail. Requires fetching transitions for the selected key on demand — reuse the Detail flow via `async_ops`. Keep the preview-before-mutation rule: the picker itself is the confirmation step, same as today.

## 8. Command palette (new)

`ctrl-k` (unbound, verified) opens a centred modal over a dimmed screen: filter input (`›` prompt, existing caret style) + grouped results. Groups in order: **on {KEY}** (transitions listed inline as `Transition KEY → Status`, assign, comment, copy key/URL, open in browser), **view** (flip view, sort, filter, tree, quick view, board), **app** (refresh, mouse, Jax, field mapping, about, help). Every row shows its direct keybinding right-aligned — the palette teaches the fast path. Fuzzy substring match, matched chars cyan bold. `↑↓` choose, `⏎` run, `esc` close. Actions must dispatch through the same functions the direct keys call. Degrade by truncating labels, never by dropping actions.

## 9. Jax 🦦🍁

Non-negotiable: all six scenes survive (wave, sleep, read-specs, fish, party, otter). Additions:

- **Mood line:** one faint line under the art (`mood: happy to see you`, `mood: five more minutes`, …) — one per scene, see mockup JS for copy.
- **Reactive moments:** transition-to-Done and successful edit/comment trigger the party scene for a few seconds; use `MAPLE` for confetti accents.
- **Mini-Jax (narrow):** when the floating 30×8 box would overlap content (< ~90 cols), Jax docks into the footer's right side as `●‿● jax {scene-emoji}`, cycling scene emoji in place. Activating him (`J`, or click in mouse mode) pops the full box out **and it stays out** — persisting across screen switches — until toggled again. Hidden on Welcome/Edit/About as today.
- Welcome Jax unchanged (blink + 🍁/🍂 swap).

## 10. Keybinding audit (scan of `src/keys.rs` vs `src/ui/help.rs`, 2026-07-15)

Fix these regardless of the rest of the refresh:

| Finding | Fix |
|---|---|
| `g` (go Home) bound but absent from help overlay | add to help |
| `l` (go List) bound but absent from help | add to help |
| `PgUp`/`PgDn` (±8 rows; board lane jump) absent from help | add to help |
| Preview applies on `⏎` as well as `y` (`keys.rs:89`) but footer/preview copy only says `y` | mention `⏎` |
| Board supports vim `hjkl` (`keys.rs:158-161`) — not hinted anywhere | help note "vim keys work" |

New bindings introduced by this spec (all verified unbound today): `<`/`>` cycle view, `x` fold facts panel (Detail, narrow), `ctrl-k` command palette. Conflicts checked: `{`/`}` (links), `[`/`]` (comments), `h`/`l` (back/list vs board vim-nav) are context-scoped and unaffected.

## 11. Responsive breakpoints

Cell-count driven, computed from the terminal `Rect` each frame (plain `Layout` switches, no new machinery):

| Width | Behaviour |
|---|---|
| ≥ 110 cols | full layout: Home rail, Detail side rail, 4-up board |
| 90–110 cols | drop list columns one at a time (assignee → type → updated); Detail rail narrows; sync pill shortens |
| < 90 cols | stacked Home strips, Detail facts panel, board column pager, mini-Jax, two-line selected rows |
| height < ~30 rows | recent line hides; glance tiles drop to 2; quick view caps at 40% height |

## 12. What must stay true (from CLAUDE.md, restated as acceptance criteria)

- ADF rendering is untouched — every change here is layout/colour; raw Markdown never enters a Jira field.
- Demo mode renders every new layout fully (breakpoints must be exercised by `tests/render.rs` via `TestBackend` at at least 120×34 and 84×46).
- Live-mode failures still fall back cache → demo, never crash; the sync pill states which source is showing.
- Mouse mode stays opt-in; Shift-drag native selection unaffected; mini-Jax click only active in mouse mode.
- Preview before any mutating call, including palette-dispatched and board/list transitions.
- Token file stays `0600`; welcome copy keeps saying so.
- Canadian spelling in all new UI copy and comments ("colour").
- All three CI feature sets stay green: `default`, `--no-default-features`, `--all-features`.

## 13. Test guidance

Extend `tests/render.rs` with `TestBackend` snapshots per screen at both reference sizes; assert on structural markers (breadcrumb text, column header presence/absence, tree guide glyphs `├─`/`└─`, footer single-row invariant, pager tab strip) rather than full-buffer equality. Unit-test the footer group-drop function and the column-drop order as pure logic in `src/ui`. The keybinding audit items are asserted by a test that walks the help rows and checks every bound key appears (a small registry of `(key, description, screens)` would make help, footer hints, and the palette all render from one table — recommended, and it prevents this audit from rotting).
