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

/// SPEC.md §11: "< 90 cols: stacked Home strips" — same threshold as
/// Detail/Board's narrow cutoff.
pub(crate) fn home_layout_for_width(width: u16) -> HomeLayout {
    if width >= 90 {
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
    fn wide_at_and_above_90() {
        assert_eq!(home_layout_for_width(90), HomeLayout::Wide);
        assert_eq!(home_layout_for_width(200), HomeLayout::Wide);
    }

    #[test]
    fn narrow_just_below_90() {
        assert_eq!(home_layout_for_width(89), HomeLayout::Narrow);
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
