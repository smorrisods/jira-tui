//! The persistent recent-issues navigation strip: a single-row, borderless
//! band of lineage-tinted chips between the body and the footer (the
//! footer itself is untouched). Shows every issue in
//! `app::history::NavHistory`'s forest and lets you click straight back to
//! any of them — see that module's doc comment for the underlying tree
//! model this projects. `strip_layout` is the shared source of truth for
//! both rendering and click hit-testing (`app::mouse`), so the two can
//! never disagree about where a chip actually is.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, NavEntry, Screen};

use super::{accent, accent2, chip, current_chip, muted, ok, task_blue, warn};

/// Below this frame height the strip hides entirely — lower than Home's own
/// `home::SHORT_HEIGHT` (30) since the strip costs a single row, not a
/// multi-row card, so it can afford to stick around longer as the terminal
/// shrinks.
pub(crate) const STRIP_MIN_HEIGHT: u16 = 20;

const LABEL: &str = " recent  ";
const SAME_LINEAGE_SEP: &str = " ";
const NEW_LINEAGE_SEP: &str = "  ·  ";
/// Reserved width for a trailing "⋯ +N" overflow marker — generous enough
/// for any realistic node count under `history::NAV_CAP`.
const OVERFLOW_MARKER_BUDGET: usize = 6;

const LINEAGE_PALETTE: [fn() -> Color; 5] = [accent, accent2, ok, warn, task_blue];

/// A lineage's stable colour, keyed by its root node's id — recycles past 5
/// concurrent lineages, which is an acceptable trade at `history::NAV_CAP`'s
/// size.
pub(crate) fn lineage_colour(lineage: u64) -> Color {
    LINEAGE_PALETTE[(lineage as usize) % LINEAGE_PALETTE.len()]()
}

/// Whether the strip should render for the app's current screen/frame size:
/// every browsing screen, hidden on modal/compose/full-screen-picker
/// screens and once the forest is empty or the terminal's too short.
pub(crate) fn nav_strip_visible(app: &App, frame_height: u16) -> bool {
    if frame_height < STRIP_MIN_HEIGHT || app.nav.is_empty() {
        return false;
    }
    matches!(
        app.screen,
        Screen::Home | Screen::List | Screen::Detail | Screen::Board | Screen::Release
    )
}

/// The strip's rendered spans plus each visible chip's `[start, end)`
/// char-offset range and issue key, for click hit-testing.
pub(crate) struct StripLayout {
    pub spans: Vec<Span<'static>>,
    pub hits: Vec<(usize, usize, String)>,
}

/// Build the strip's layout for `entries` at `width` columns: lineage bands
/// stay contiguous (a plain space between same-lineage chips, a muted `·`
/// between different lineages — `entries()` already orders lineages
/// most-recently-visited-first and nodes MRU within a lineage), and whole
/// trailing chips drop behind a muted `⋯ +N` marker once the line would
/// overflow `width` — the least-recently-visited lineages first, since
/// `entries()`'s ordering puts them last. Pure and reused by both the
/// renderer and `app::mouse`'s hit-testing, so the two can never disagree
/// about where a chip landed.
pub(crate) fn strip_layout(entries: &[NavEntry], width: usize) -> StripLayout {
    let mut spans = vec![Span::styled(LABEL, Style::default().fg(muted()))];
    let mut hits = Vec::with_capacity(entries.len());
    let mut col = LABEL.chars().count();
    let mut last_lineage: Option<u64> = None;
    let mut shown = 0usize;

    for entry in entries {
        let sep = match last_lineage {
            None => "",
            Some(prev) if prev == entry.lineage => SAME_LINEAGE_SEP,
            Some(_) => NEW_LINEAGE_SEP,
        };
        let colour = lineage_colour(entry.lineage);
        let chip_span = if entry.current {
            current_chip(&entry.key, colour)
        } else {
            chip(&entry.key, colour)
        };
        let piece_width = sep.chars().count() + chip_span.content.chars().count();

        let will_truncate = shown + 1 < entries.len();
        let budget = if will_truncate {
            width.saturating_sub(OVERFLOW_MARKER_BUDGET)
        } else {
            width
        };
        if col + piece_width > budget {
            break;
        }

        if !sep.is_empty() {
            spans.push(Span::styled(sep, Style::default().fg(muted())));
            col += sep.chars().count();
        }
        let start = col;
        col += chip_span.content.chars().count();
        hits.push((start, col, entry.key.clone()));
        spans.push(chip_span);
        last_lineage = Some(entry.lineage);
        shown += 1;
    }

    if shown < entries.len() {
        let marker = format!(" ⋯ +{}", entries.len() - shown);
        // Only the chips above were checked against the reserved
        // `OVERFLOW_MARKER_BUDGET` — this is the one place that budget is
        // actually spent, so it still needs its own fit check: a pane too
        // narrow even for the label alone (`shown == 0`) would otherwise
        // push a marker past `width` with nothing to show for it.
        if col + marker.chars().count() <= width {
            spans.push(Span::styled(marker, Style::default().fg(muted())));
        }
    }

    StripLayout { spans, hits }
}

pub(crate) fn draw_nav_strip(f: &mut Frame, app: &App, area: Rect) {
    app.nav_strip_area.set(area);
    let entries = app.nav.entries();
    let layout = strip_layout(&entries, area.width as usize);
    f.render_widget(Paragraph::new(Line::from(layout.spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, lineage: u64, current: bool) -> NavEntry {
        NavEntry {
            key: key.to_string(),
            lineage,
            current,
        }
    }

    #[test]
    fn same_lineage_chips_get_a_plain_space_different_lineages_get_a_dot() {
        let entries = vec![
            entry("A", 1, false),
            entry("B", 1, true),
            entry("C", 2, false),
        ];
        let layout = strip_layout(&entries, 200);
        let text: String = layout.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains(" A "),
            "non-current chips use the plain `chip()` padding"
        );
        assert!(
            text.contains("  ·  "),
            "a lineage change should separate with the muted dot, not a bare space: {text:?}"
        );
    }

    #[test]
    fn hits_cover_exactly_the_chips_that_were_shown_without_overlap() {
        let entries = vec![
            entry("A", 1, false),
            entry("B", 2, true),
            entry("C", 3, false),
        ];
        let layout = strip_layout(&entries, 200);
        assert_eq!(layout.hits.len(), entries.len());
        let keys: Vec<&str> = layout.hits.iter().map(|(_, _, k)| k.as_str()).collect();
        assert_eq!(keys, vec!["A", "B", "C"]);
        for w in layout.hits.windows(2) {
            let (_, prev_end, _) = w[0];
            let (next_start, _, _) = w[1];
            assert!(
                prev_end <= next_start,
                "chip hit ranges must not overlap: {:?}",
                layout.hits
            );
        }
    }

    #[test]
    fn truncates_trailing_chips_behind_an_overflow_marker_when_too_narrow() {
        let entries: Vec<NavEntry> = (0..10)
            .map(|i| entry(&format!("ISSUE-{i}"), i as u64, i == 0))
            .collect();
        let layout = strip_layout(&entries, 30);
        assert!(
            layout.hits.len() < entries.len(),
            "a narrow width should drop some trailing chips"
        );
        let text: String = layout.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains('⋯'),
            "dropped chips should leave an overflow marker: {text:?}"
        );
    }

    #[test]
    fn lineage_colour_is_stable_per_lineage_and_recycles_past_five() {
        assert_eq!(lineage_colour(0), lineage_colour(0));
        assert_eq!(
            lineage_colour(0),
            lineage_colour(LINEAGE_PALETTE.len() as u64),
            "the palette should recycle once lineages outnumber it"
        );
    }

    #[test]
    fn nav_strip_visible_hides_below_the_height_threshold_and_on_non_browsing_screens() {
        let mut app = App::new(true);
        let key = app.issues[0].key.clone();
        app.open_by_key(&key); // populate the forest so emptiness isn't the reason it's hidden

        app.screen = Screen::Home;
        assert!(nav_strip_visible(&app, STRIP_MIN_HEIGHT));
        assert!(!nav_strip_visible(&app, STRIP_MIN_HEIGHT - 1));

        app.screen = Screen::Edit;
        assert!(
            !nav_strip_visible(&app, 40),
            "a compose screen should hide the strip regardless of height"
        );
    }

    #[test]
    fn nav_strip_visible_hides_on_a_fresh_session_with_no_history_yet() {
        let app = App::new(true);
        assert!(!nav_strip_visible(&app, 40));
    }
}
