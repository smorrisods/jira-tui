//! Pure breakpoint logic for the Detail screen (SPEC.md §6/§11): whether the
//! screen is wide enough for the two-column layout (main column + side
//! rail), and how wide that rail should be. Mirrors `list_columns.rs`'s
//! shape — a small, unit-tested function the renderer and the nav code both
//! call, rather than each re-deriving the same width check.

use ratatui::text::Line;

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

/// How many on-screen rows `lines` will actually occupy once wrapped (via
/// `Wrap { trim: false }`) at `width` columns — used to size a side-rail
/// panel's `Constraint::Length` from the real wrapped row count instead of
/// the logical (unwrapped) line count, which under-allocates height and
/// silently clips trailing content the moment any line wraps.
pub(crate) fn wrapped_row_count(lines: &[Line], width: u16) -> u16 {
    if width == 0 {
        return lines.len() as u16;
    }
    let rows: usize = lines
        .iter()
        .map(|line| {
            let w = line.width();
            if w == 0 {
                1
            } else {
                w.div_ceil(width as usize)
            }
        })
        .sum();
    rows.try_into().unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

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

    fn line(text: &str) -> Line<'static> {
        Line::from(Span::raw(text.to_string()))
    }

    #[test]
    fn short_lines_take_one_row_each() {
        let lines = vec![line("hello"), line("")];
        assert_eq!(wrapped_row_count(&lines, 10), 2);
    }

    #[test]
    fn a_line_exactly_at_width_does_not_wrap() {
        let lines = vec![line("1234567890")];
        assert_eq!(wrapped_row_count(&lines, 10), 1);
    }

    #[test]
    fn a_line_one_over_width_wraps_to_two_rows() {
        let lines = vec![line("12345678901")];
        assert_eq!(wrapped_row_count(&lines, 10), 2);
    }

    #[test]
    fn a_zero_width_area_falls_back_to_the_logical_line_count() {
        let lines = vec![line("anything"), line("at all")];
        assert_eq!(wrapped_row_count(&lines, 0), 2);
    }
}
