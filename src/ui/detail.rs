//! The full issue Detail screen.

use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

use super::{card, ACCENT};

pub(crate) fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let Some(detail) = app.detail.as_ref() else {
        f.render_widget(
            Paragraph::new("No issue loaded").block(card("  detail  ", ACCENT)),
            area,
        );
        return;
    };

    let block = card(&format!("  {}  ", detail.key), ACCENT);
    let inner = block.inner(area);
    app.detail_area.set(inner);
    f.render_widget(block, area);

    let rendered = crate::render::issue_detail_lines(detail);
    let para = Paragraph::new(ratatui::text::Text::from(rendered.lines))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));
    f.render_widget(para, inner);
}
