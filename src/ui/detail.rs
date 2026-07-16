//! The full issue Detail screen (SPEC.md §6): a wide two-column layout
//! (scrollable main column + a static four-panel side rail) at ≥ ~90 cols,
//! or a single scrollable column with a foldable facts panel below that —
//! see `detail_columns::detail_layout_for_width`.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::domain::IssueDetail;
use crate::render::{self, DetailPane, Panel};

use super::detail_columns::{
    detail_layout_for_width, rail_width_for, wrapped_row_count, DetailLayout,
};
use super::{accent, accent2, card};

pub(crate) fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let Some(detail) = app.detail.as_ref() else {
        f.render_widget(
            Paragraph::new("No issue loaded").block(card("  detail  ", accent())),
            area,
        );
        return;
    };

    let block = card(&format!("  {}  ", detail.key), accent());
    let inner = block.inner(area);
    app.detail_area.set(inner);
    f.render_widget(block, area);

    let updated = app.issue_updated(&detail.key).to_string();
    let current_user = app.current_user_display();

    match detail_layout_for_width(inner.width) {
        DetailLayout::Wide => draw_wide(f, app, inner, detail, &current_user, &updated),
        DetailLayout::Narrow => draw_narrow(f, app, inner, detail, &current_user, &updated),
    }
}

fn draw_wide(
    f: &mut Frame,
    app: &App,
    area: Rect,
    detail: &IssueDetail,
    current_user: &str,
    updated: &str,
) {
    let mut wide = render::wide_detail(detail, current_user, updated);
    if let Some(target) = render::wide_detail_links(&wide)
        .get(app.link_index)
        .cloned()
    {
        let lines = match target.pane {
            DetailPane::Identity => &mut wide.identity.lines,
            DetailPane::Main => &mut wide.main.lines,
            DetailPane::Workflow => &mut wide.workflow.lines,
            DetailPane::Meta => &mut wide.meta.lines,
            DetailPane::Links => &mut wide.links.lines,
            DetailPane::Children => &mut wide.children.lines,
        };
        render::highlight_target(lines, &target);
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),
            Constraint::Length(rail_width_for(area.width)),
        ])
        .split(area);

    // The identity block's summary line has no fixed length — a long one
    // needs more than its 2 logical lines once wrapped at the main
    // column's width, the same under-allocation bug the rail panels had
    // (see `wrapped_row_count`'s own doc comment): sizing from the raw
    // line count and never calling `.wrap()` on the Paragraph meant a long
    // summary was silently hard-clipped mid-word instead of wrapping.
    let identity_height = wrapped_row_count(&wide.identity.lines, cols[0].width);
    let main_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(identity_height), Constraint::Min(3)])
        .split(cols[0]);
    f.render_widget(
        Paragraph::new(wide.identity.lines).wrap(Wrap { trim: false }),
        main_rows[0],
    );
    app.detail_main_area.set(main_rows[1]);
    f.render_widget(
        Paragraph::new(wide.main.lines)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        main_rows[1],
    );

    let workflow_title = "workflow · t to change".to_string();
    let meta_title = "people & meta".to_string();
    let links_title = "links".to_string();
    let children_title = if detail.children.is_empty() {
        "children".to_string()
    } else {
        format!("children · {}", detail.children.len())
    };
    draw_rail(
        f,
        cols[1],
        [
            (workflow_title, accent(), wide.workflow),
            (meta_title, accent(), wide.meta),
            (links_title, accent2(), wide.links),
            (children_title, accent2(), wide.children),
        ],
    );
}

/// The static side rail: four bordered mini-panels (matching this app's
/// established "titled card" look everywhere else — quick view, Board's
/// cards, the outer Detail card itself), sized to their own wrapped content
/// (via `wrapped_row_count`, against the *inner* content width now that a
/// border eats 2 columns — the logical line count alone under-allocates
/// height once a line wraps, silently clipping trailing lines) plus 2 rows
/// for the top/bottom border, except the last panel, which takes whatever's
/// left. Deliberately non-scrolling — panels are short/bounded, and
/// clipping on genuine overflow (more content than the rail area has room
/// for at all) is an accepted scope cut for this phase (see the module
/// doc's plan reference).
fn draw_rail(f: &mut Frame, area: Rect, panels: [(String, Color, Panel); 4]) {
    let last = panels.len() - 1;
    let content_width = area.width.saturating_sub(2);
    let constraints: Vec<Constraint> = panels
        .iter()
        .enumerate()
        .map(|(i, (_, _, panel))| {
            if i == last {
                Constraint::Min(3)
            } else {
                Constraint::Length(wrapped_row_count(&panel.lines, content_width) + 2)
            }
        })
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (i, (title, colour, panel)) in panels.into_iter().enumerate() {
        draw_rail_panel(f, rows[i], &title, colour, panel);
    }
}

fn draw_rail_panel(f: &mut Frame, area: Rect, title: &str, colour: Color, panel: Panel) {
    let block = card(&format!("  {title}  "), colour);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(panel.lines).wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_narrow(
    f: &mut Frame,
    app: &App,
    area: Rect,
    detail: &IssueDetail,
    current_user: &str,
    updated: &str,
) {
    let mut narrow = render::narrow_detail(detail, current_user, updated, app.facts_folded);
    if let Some(target) = narrow.lines.links.get(app.link_index).cloned() {
        render::highlight_target(&mut narrow.lines.lines, &target);
    }
    app.detail_main_area.set(area);
    let para = Paragraph::new(narrow.lines.lines)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, area);
}
