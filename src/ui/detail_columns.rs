//! Pure breakpoint logic for the Detail screen (SPEC.md §6/§11): whether the
//! screen is wide enough for the two-column layout (main column + side
//! rail), and how wide that rail should be. Mirrors `list_columns.rs`'s
//! shape — a small, unit-tested function the renderer and the nav code both
//! call, rather than each re-deriving the same width check.

/// Which of the two Detail layouts a given width should use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DetailLayout {
    /// Two columns: scrollable main column (identity/description/activity)
    /// plus a static side rail (workflow/people & meta/links/children).
    Wide,
    /// One column, order: identity → facts panel → description → linked
    /// panel → activity.
    Narrow,
}

/// SPEC.md §11: "< 90 cols: ... Detail facts panel".
pub(crate) fn detail_layout_for_width(width: u16) -> DetailLayout {
    if width >= 90 {
        DetailLayout::Wide
    } else {
        DetailLayout::Narrow
    }
}

/// SPEC.md §11: "90–110 cols: ... Detail rail narrows". Below 110 the rail
/// shrinks from ~34 to ~26 columns rather than disappearing outright — it
/// only actually goes away once `detail_layout_for_width` switches to
/// `Narrow` at 90.
pub(crate) fn rail_width_for(width: u16) -> u16 {
    if width >= 110 {
        34
    } else {
        26
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_at_and_above_90() {
        assert_eq!(detail_layout_for_width(90), DetailLayout::Wide);
        assert_eq!(detail_layout_for_width(200), DetailLayout::Wide);
    }

    #[test]
    fn narrow_just_below_90() {
        assert_eq!(detail_layout_for_width(89), DetailLayout::Narrow);
    }

    #[test]
    fn rail_full_width_at_and_above_110() {
        assert_eq!(rail_width_for(110), 34);
        assert_eq!(rail_width_for(200), 34);
    }

    #[test]
    fn rail_narrows_just_below_110() {
        assert_eq!(rail_width_for(109), 26);
        assert_eq!(rail_width_for(90), 26);
    }
}
