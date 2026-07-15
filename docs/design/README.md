# UI refresh proposal

Two sets of static HTML mockups for a refreshed jira-tui UI — open either in
any browser (no build step, no network):

- `ui-refresh.html` — the landscape set (wide terminals, ≥ 110 columns)
- `ui-refresh-portrait.html` — the portrait companion set (tall/narrow
  terminals, < 90 columns), showing how the same UI restacks: side rails
  become strips, the detail rail folds into a "facts" panel, the board pages
  one status column at a time, and Jax docks as a mini companion in every
  footer

Press `1`–`6` or use the tabs to switch between the six mocked screens:

1. **Welcome** — three onboarding paths as cards, with a recommended default
2. **Home** — context rail with glance bars and a recent-issues panel
3. **List + quick view** — aligned columns, status chips, split quick view
4. **Detail** — two-column layout: content left, metadata rail right
5. **Board** — bordered cards with assignee/age, WIP markers, ghost cells
6. **Command palette** — a proposed `ctrl-k` action palette (not yet in the app)

## Design intent

- Keep the cyan/magenta identity and the header / body / footer chrome.
- Add one new accent — **maple** (`#E8834A`, from Jax's leaf) — used only for
  "you are here": row selection, board-card selection, and Jax moments.
- Replace packed title/footer strings with structure: breadcrumbs in the
  header, grouped key hints (`nav` / `view` / `act` / `go`) in the footer.
- Status and type move from coloured text to tinted chips; the same semantic
  colours, easier to scan down a column.
- Everything shown maps onto existing ratatui widgets; the command palette is
  the only genuinely new capability proposed.

Jax is retained and promoted: all six companion scenes survive, and the
companion gains a mood line. Click him in the mockup.
