//! Pure breakpoint logic for the quick-view panel (SPEC.md §4/§11): whether
//! it's wide enough for the description-plus-meta-grid layout, and how wide
//! the meta grid should be. Mirrors `detail_columns.rs`'s shape.

/// Which of the two quick-view layouts a given width should use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum QuickViewLayout {
    /// Two columns: description excerpt (left) beside a compact meta grid
    /// (right).
    Wide,
    /// One column: chips row, then inline `key value` pairs, then the
    /// description excerpt.
    Narrow,
}

/// SPEC.md §4: "Wide (>= ~100 cols)" — a distinct threshold from Detail's 90
/// and Board's 90, since the meta grid needs more spare width than either.
pub(crate) fn quick_view_layout_for_width(width: u16) -> QuickViewLayout {
    if width >= 100 {
        QuickViewLayout::Wide
    } else {
        QuickViewLayout::Narrow
    }
}

/// The wide layout's meta-grid column width — a little more room on wider
/// terminals so long values (labels, parent keys) wrap less eagerly.
pub(crate) fn meta_width_for(width: u16) -> u16 {
    if width >= 130 {
        32
    } else {
        26
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_at_and_above_100() {
        assert_eq!(quick_view_layout_for_width(100), QuickViewLayout::Wide);
        assert_eq!(quick_view_layout_for_width(200), QuickViewLayout::Wide);
    }

    #[test]
    fn narrow_just_below_100() {
        assert_eq!(quick_view_layout_for_width(99), QuickViewLayout::Narrow);
    }

    #[test]
    fn meta_width_widens_at_130() {
        assert_eq!(meta_width_for(129), 26);
        assert_eq!(meta_width_for(130), 32);
    }
}
