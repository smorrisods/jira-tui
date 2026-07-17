//! Pure breakpoint logic for the Home screen (SPEC.md §5/§11): whether the
//! rail renders as three stacked cards or collapses to narrow strips, and
//! the glance tiles' proportion-bar math. Mirrors `detail_columns.rs`'s
//! shape.

/// Which of the two Home rail layouts a given width should use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum HomeLayout {
    /// A ~32-col left rail of three stacked cards (context/glance/recent)
    /// beside the shared list panel.
    Wide,
    /// The rail collapses to strips stacked above the list: one context
    /// line, a 4-up glance tile row, and (space permitting) a one-line
    /// recent strip.
    Narrow,
}

/// The wide rail claims 32% of the width, leaving the shared list panel only
/// 68% — so SPEC.md §11's ">= 90 cols: wide rail" isn't enough on its own to
/// guarantee the list panel actually looks "wide": at 90 cols the list panel
/// is only ~59 cols inside its border, well under `list_columns`'s own
/// thresholds (90 to avoid the two-line row split, 103 for every optional
/// column). Below this width the rail would sit next to a column-starved,
/// two-line list — worse than just collapsing the rail into strips and
/// giving the list the full row. 154 is the smallest total width at which
/// ratatui's `Percentage(68)` split leaves the list panel's own inner width
/// at >= 103 (every optional column shown, never two-line) — see
/// `list_columns::column_set_for_width`; `list_wide_enough_for_all_columns`
/// below pins this pairing so the two breakpoints can't drift apart again.
const WIDE_RAIL_MIN_TOTAL_WIDTH: u16 = 154;

pub(crate) fn home_layout_for_width(width: u16) -> HomeLayout {
    if width >= WIDE_RAIL_MIN_TOTAL_WIDTH {
        HomeLayout::Wide
    } else {
        HomeLayout::Narrow
    }
}

/// Proportion-bar fill, scaled against the largest of the glance counts
/// shown this frame (not a fixed denominator) so the bars stay meaningful
/// regardless of magnitude — a lone nonzero count always shows a full bar,
/// and all-zero counts show empty bars rather than dividing by zero.
/// `cells` is the bar's total cell count.
pub(crate) fn bar_fill(value: usize, max: usize, cells: u16) -> u16 {
    if max == 0 || value == 0 {
        return 0;
    }
    ((value as f64 / max as f64) * cells as f64)
        .ceil()
        .min(cells as f64) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_at_and_above_the_threshold() {
        assert_eq!(
            home_layout_for_width(WIDE_RAIL_MIN_TOTAL_WIDTH),
            HomeLayout::Wide
        );
        assert_eq!(home_layout_for_width(200), HomeLayout::Wide);
    }

    #[test]
    fn narrow_just_below_the_threshold() {
        assert_eq!(
            home_layout_for_width(WIDE_RAIL_MIN_TOTAL_WIDTH - 1),
            HomeLayout::Narrow
        );
    }

    /// Pins the pairing this module's width threshold depends on: the wide
    /// rail must never turn on unless the list panel next to it (68% of the
    /// same total width, per `home::draw_wide`'s split) already qualifies
    /// for every optional column in `list_columns::column_set_for_width`.
    /// If either module's thresholds move, this is the test that should
    /// catch the drift.
    #[test]
    fn list_wide_enough_for_all_columns() {
        use ratatui::layout::{Constraint, Direction, Layout, Rect};

        let area = Rect::new(0, 0, WIDE_RAIL_MIN_TOTAL_WIDTH, 40);
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(area);
        let list_inner_width = cols[1].width.saturating_sub(2);
        let columns = crate::ui::list_columns::column_set_for_width(list_inner_width);
        assert!(columns.assignee && columns.type_chip && columns.updated && !columns.two_line);

        let area = Rect::new(0, 0, WIDE_RAIL_MIN_TOTAL_WIDTH - 1, 40);
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(area);
        let list_inner_width = cols[1].width.saturating_sub(2);
        let columns = crate::ui::list_columns::column_set_for_width(list_inner_width);
        assert!(
            !(columns.assignee && columns.type_chip && columns.updated && !columns.two_line),
            "the threshold should be the smallest width at which the list panel qualifies"
        );
    }

    #[test]
    fn bar_fill_is_zero_when_every_count_is_zero() {
        assert_eq!(bar_fill(0, 0, 4), 0);
    }

    #[test]
    fn bar_fill_is_zero_for_a_zero_value_even_with_a_nonzero_max() {
        assert_eq!(bar_fill(0, 10, 4), 0);
    }

    #[test]
    fn bar_fill_is_full_when_value_equals_max() {
        assert_eq!(bar_fill(10, 10, 4), 4);
    }

    #[test]
    fn bar_fill_scales_proportionally_and_rounds_up() {
        // 1 of 4 -> at least one lit cell, never fully empty for a nonzero
        // value.
        assert_eq!(bar_fill(1, 4, 4), 1);
        assert_eq!(bar_fill(2, 4, 4), 2);
    }

    #[test]
    fn bar_fill_never_exceeds_the_cell_count() {
        assert_eq!(bar_fill(100, 100, 4), 4);
    }
}
