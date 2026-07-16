//! Pure column-drop logic for the work list (SPEC.md §3/§11): which of the
//! optional columns (assignee, type chip, updated) fit at a given width,
//! and whether the terminal is narrow enough that the selected row needs a
//! second line to show what got dropped. Key, status, and summary never
//! drop — they're not represented here because they're unconditional.

/// Which optional columns fit at the current width. `two_line` is SPEC.md
/// §3's "below ~90 cols the selected row grows a second line carrying
/// exactly the dropped columns" — it goes true at the same boundary
/// `updated` drops, since by then every optional column is already gone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ColumnSet {
    pub assignee: bool,
    pub type_chip: bool,
    pub updated: bool,
    pub two_line: bool,
}

/// Four thresholds spaced across SPEC.md §11's 90–110 "drop list columns
/// one at a time" band, each optional column getting its own distinct drop
/// point: ≥110 all columns, 103–109 drop assignee, 96–102 drop assignee +
/// type, 90–95 drop all three, <90 also switches the selected row to two
/// lines.
pub(crate) fn column_set_for_width(width: u16) -> ColumnSet {
    ColumnSet {
        assignee: width >= 103,
        type_chip: width >= 96,
        updated: width >= 90,
        two_line: width < 90,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_columns_present_at_full_width() {
        let c = column_set_for_width(110);
        assert!(c.assignee && c.type_chip && c.updated && !c.two_line);
    }

    #[test]
    fn assignee_drops_first_just_below_103() {
        let above = column_set_for_width(103);
        let below = column_set_for_width(102);
        assert!(above.assignee);
        assert!(!below.assignee);
        assert!(below.type_chip && below.updated);
    }

    #[test]
    fn type_chip_drops_next_just_below_96() {
        let above = column_set_for_width(96);
        let below = column_set_for_width(95);
        assert!(above.type_chip);
        assert!(!below.type_chip);
        assert!(!below.assignee, "assignee should already be gone by 95");
        assert!(below.updated);
    }

    #[test]
    fn updated_and_two_line_flip_together_below_90() {
        let above = column_set_for_width(90);
        let below = column_set_for_width(89);
        assert!(above.updated && !above.two_line);
        assert!(!below.updated && below.two_line);
        assert!(!below.assignee && !below.type_chip);
    }
}
