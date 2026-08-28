//! The full issue Detail screen (SPEC.md §6): a wide two-column layout
//! (scrollable main column + a static five-panel side rail) at ≥ ~90 cols,
//! or a single scrollable column with a foldable facts panel below that —
//! see `detail_columns::detail_layout_for_width`.

use std::cell::Cell;

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
    // Computed before `render::wide_detail` (not after, as the two-column
    // split might suggest) — `wide_detail` needs the main column's actual
    // width up front so a description/comment bar can be pre-wrapped to
    // survive line wrapping (see `adf::render`'s doc comment).
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(40),
            Constraint::Length(rail_width_for(area.width)),
        ])
        .split(area);

    let mut wide = app.with_detail_media_sizing(cols[0].width, |media| {
        render::wide_detail(detail, current_user, updated, cols[0].width as usize, media)
    });
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
            DetailPane::Attachments => &mut wide.attachments.lines,
        };
        render::highlight_target(lines, &target);
    }

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
    // Computed from `&wide.main.lines` (a borrow, not a move) before the
    // `Paragraph::new` below consumes them — see `image_paint_offsets`'s
    // doc comment for why the visual-row math needs the actual line
    // content, not just the placement list.
    #[cfg(feature = "images")]
    let image_paints = image_paint_offsets(
        &wide.main.lines,
        &wide.image_placements,
        main_rows[1].width,
        app.detail_scroll,
    );
    f.render_widget(
        Paragraph::new(wide.main.lines)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        main_rows[1],
    );
    #[cfg(feature = "images")]
    paint_inline_images(f, app, main_rows[1], image_paints, Some(detail));

    let workflow_title = "workflow · t to change".to_string();
    let meta_title = "people & meta".to_string();
    let links_title = "links".to_string();
    let children_title = if detail.children.is_empty() {
        "children".to_string()
    } else {
        format!("children · {}", detail.children.len())
    };
    let attachments_title = if detail.attachments.is_empty() {
        "attachments".to_string()
    } else {
        format!("attachments · {}", detail.attachments.len())
    };
    draw_rail(
        f,
        cols[1],
        [
            (
                workflow_title,
                accent(),
                wide.workflow,
                &app.detail_workflow_area,
            ),
            (meta_title, accent(), wide.meta, &app.detail_meta_area),
            (links_title, accent2(), wide.links, &app.detail_links_area),
            (
                children_title,
                accent2(),
                wide.children,
                &app.detail_children_area,
            ),
            (
                attachments_title,
                accent2(),
                wide.attachments,
                &app.detail_attachments_area,
            ),
        ],
    );
}

/// The static side rail: five bordered mini-panels (matching this app's
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
fn draw_rail(f: &mut Frame, area: Rect, panels: [(String, Color, Panel, &Cell<Rect>); 5]) {
    let last = panels.len() - 1;
    let content_width = area.width.saturating_sub(2);
    let constraints: Vec<Constraint> = panels
        .iter()
        .enumerate()
        .map(|(i, (_, _, panel, _))| {
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
    for (i, (title, colour, panel, area_cell)) in panels.into_iter().enumerate() {
        draw_rail_panel(f, rows[i], &title, colour, panel, area_cell);
    }
}

/// `area_cell` records this panel's inner (post-border) area for mouse
/// hit-testing (`app::mouse::link_at`) — the panel is deliberately
/// non-scrolling (see `draw_rail`'s doc comment), so unlike
/// `detail_main_area` no separate scroll-Rect is needed.
fn draw_rail_panel(
    f: &mut Frame,
    area: Rect,
    title: &str,
    colour: Color,
    panel: Panel,
    area_cell: &Cell<Rect>,
) {
    let block = card(&format!("  {title}  "), colour);
    let inner = block.inner(area);
    area_cell.set(inner);
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
    let mut narrow = app.with_detail_media_sizing(area.width, |media| {
        render::narrow_detail(
            detail,
            current_user,
            updated,
            app.facts_folded,
            area.width as usize,
            media,
        )
    });
    if let Some(target) = narrow.lines.links.get(app.link_index).cloned() {
        render::highlight_target(&mut narrow.lines.lines, &target);
    }
    app.detail_main_area.set(area);
    #[cfg(feature = "images")]
    let image_paints = image_paint_offsets(
        &narrow.lines.lines,
        &narrow.image_placements,
        area.width,
        app.detail_scroll,
    );
    let para = Paragraph::new(narrow.lines.lines)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, area);
    #[cfg(feature = "images")]
    paint_inline_images(f, app, area, image_paints, Some(detail));
}

/// Convert each `ImagePlacement`'s logical `line_start` into a *visual*
/// (post-wrap) row offset relative to the pane's current scroll position
/// (`images` feature only, Phase 3 of issue #130) — `scroll` is in the same
/// wrapped-row units `Paragraph::scroll` itself consumes (see
/// `detail_columns::wrapped_row_count`'s own doc comment), so the row count
/// of everything *before* `line_start` (via `wrapped_row_count` on that
/// prefix slice) is exactly where the reserved rows start once rendered,
/// minus `scroll` to land in on-screen coordinates. Must be called with
/// `lines` still intact — i.e. before whatever `Paragraph::new` call is
/// about to move them — since this needs the actual line content wrapping
/// was computed against, the same constraint `app::mouse::link_at`'s own
/// wrap-aware click mapping already has.
///
/// `pub(super)` (rather than file-private) since `ui::quick_view`'s own
/// paint pass reuses this verbatim (Phase 5 of issue #130) — the maths here
/// never touched `App`/Detail-specific state to begin with, just line
/// content and placements, so quick view's own `Panel`/scroll shape needs
/// nothing different.
#[cfg(feature = "images")]
pub(super) fn image_paint_offsets(
    lines: &[ratatui::text::Line<'static>],
    placements: &[crate::adf::ImagePlacement],
    width: u16,
    scroll: u16,
) -> Vec<(crate::adf::ImagePlacement, i32)> {
    placements
        .iter()
        .map(|p| {
            let prefix_end = p.line_start.min(lines.len());
            let visual_row = wrapped_row_count(&lines[..prefix_end], width);
            (p.clone(), visual_row as i32 - scroll as i32)
        })
        .collect()
}

/// Paint every still-on-screen inline image over `pane`, after its
/// scrolled `Paragraph` has already been rendered (`images` feature only,
/// Phase 3 of issue #130) — a placement whose full row range is above or
/// below the visible area (`y + rows <= 0` or `y >= pane.height`) is
/// skipped entirely rather than handed to `SlicedImage`, which is both a
/// cheap optimization and correct either way: `SlicedProtocol` clips a
/// partially-visible placement on its own. `App::sliced_inline_image_protocol`
/// returning `None` (no picker, or the decoded image was evicted from the
/// cache since sizing was computed) just leaves that reservation's rows
/// blank — never a panic, never a fallback placeholder drawn over already
/// -rendered blank rows.
///
/// `pub(super)`, and takes `detail` explicitly (rather than reading
/// `app.detail` itself) since `ui::quick_view` reuses this too (Phase 5 of
/// issue #130) — its own images come from `app.quick_view_detail()`, not
/// `app.detail`. `None` is a legitimate value here (no picker, or a document
/// whose images all resolve without ever needing an attachment-id lookup),
/// not just a placeholder for "unavailable."
#[cfg(feature = "images")]
pub(super) fn paint_inline_images(
    f: &mut Frame,
    app: &App,
    pane: Rect,
    offsets: Vec<(crate::adf::ImagePlacement, i32)>,
    detail: Option<&IssueDetail>,
) {
    for (placement, y) in offsets {
        if y + i32::from(placement.rows) <= 0 || y >= i32::from(pane.height) {
            continue;
        }
        let Some(protocol) = app.sliced_inline_image_protocol(detail, &placement) else {
            continue;
        };
        let y = y.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let position = ratatui_image::sliced::SignedPosition::from((0, y));
        f.render_widget(
            ratatui_image::sliced::SlicedImage::new(&protocol, position),
            pane,
        );
    }
}
