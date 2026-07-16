//! Pure breakpoint logic for the Board screen (SPEC.md §7/§11): whether the
//! screen is wide enough for the card grid, or narrow enough that it
//! becomes a one-column-at-a-time pager, plus the fixed card height and the
//! wide grid's per-column width budget. Mirrors `detail_columns.rs`'s
//! shape — small, unit-tested functions the renderer and the nav code both
//! call, rather than each re-deriving the same width check.

/// Bordered card height: 2 border rows + top (glyph/key/type chip[+⛔]) +
/// summary + footer (assignee/age) content lines. The narrow layout's
/// selected card grows one extra row for its neighbour-peek line — that's
/// added on top of this constant where needed, not folded in here, since
/// it's specific to exactly one card at a time.
pub(crate) const CARD_HEIGHT: u16 = 5;

/// Which Board layout a given width should use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BoardLayout {
    /// A card grid: status columns across, Epic swimlanes down.
    Wide,
    /// A one-column-at-a-time pager, still grouped by swimlane.
    Narrow,
}

/// SPEC.md §11: "< 90 cols: ... board column pager" — the same 90-col
/// threshold as Detail's wide/narrow split and the list's two-line
/// breakpoint.
pub(crate) fn board_layout_for_width(width: u16) -> BoardLayout {
    if width >= 90 {
        BoardLayout::Wide
    } else {
        BoardLayout::Narrow
    }
}

/// Per-column card width for the wide grid: a 1-column gap between adjacent
/// card columns (each card's own border already separates it visually, no
/// `│` separator needed the way the old text grid used), floored at 18 so
/// a card's top line (priority glyph + key + type chip[+⛔]) has room.
pub(crate) fn board_card_col_width(inner_width: u16, n: usize) -> u16 {
    let n = n.max(1) as u16;
    let gaps = n.saturating_sub(1);
    (inner_width.saturating_sub(gaps) / n).max(18)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_at_and_above_90() {
        assert_eq!(board_layout_for_width(90), BoardLayout::Wide);
        assert_eq!(board_layout_for_width(200), BoardLayout::Wide);
    }

    #[test]
    fn narrow_just_below_90() {
        assert_eq!(board_layout_for_width(89), BoardLayout::Narrow);
    }

    #[test]
    fn card_col_width_splits_evenly_minus_gaps() {
        // 4 columns, 3 gaps: (124 - 3) / 4 = 30.25 -> 30.
        assert_eq!(board_card_col_width(124, 4), 30);
    }

    #[test]
    fn card_col_width_floors_at_18() {
        assert_eq!(board_card_col_width(40, 4), 18);
    }

    #[test]
    fn card_col_width_handles_a_single_column() {
        // No gaps to subtract with only one column.
        assert_eq!(board_card_col_width(50, 1), 50);
        assert_eq!(board_card_col_width(50, 0), 50);
    }

    /// Regression test, ported from the old text-grid's
    /// `row_width_never_exceeds_the_pane_when_not_floor_clamped` (deleted
    /// along with `board_col_width`): the total width the wide grid actually
    /// lays out (`n` cards at `board_card_col_width` plus a 1-col gap
    /// between each, matching `Layout::horizontal(...).spacing(1)`) must
    /// never exceed the pane — unless the 18-char floor is doing the
    /// clamping, which (like the old floor) is an accepted narrow-terminal
    /// tradeoff for a wide workflow with many status columns, not a bug.
    #[test]
    fn card_grid_width_never_exceeds_the_pane_when_not_floor_clamped() {
        for n in 1..=8usize {
            for inner_width in [90u16, 96, 110, 140, 200] {
                let gaps = n.saturating_sub(1) as u16;
                let unclamped = inner_width.saturating_sub(gaps) / (n.max(1) as u16);
                if unclamped < 18 {
                    continue; // the floor is clamping; overflow is expected here
                }
                let col_width = board_card_col_width(inner_width, n);
                let total = n as u16 * col_width + gaps;
                assert!(
                    total <= inner_width,
                    "n={n} inner_width={inner_width}: grid width {total} exceeds pane"
                );
            }
        }
    }
}
