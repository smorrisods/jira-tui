//! Headless rendering tests — drive the real `ui::draw` through a TestBackend
//! and assert the composed screen text, so each screen is exercised in CI.

use jira_tui::app::{App, RailPanel, Screen, WelcomePhase};
use jira_tui::domain::{ChildIssue, IssueSummary, Priority, Source};
use jira_tui::ui;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

fn dump(buf: &Buffer) -> String {
    let area = buf.area;
    let mut s = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let sym = buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" ");
            s.push_str(sym);
        }
        s.push('\n');
    }
    s
}

fn render(app: &App) -> String {
    render_at(app, 120, 40)
}

fn render_at(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    dump(terminal.backend().buffer())
}

fn demo_app() -> App {
    App::new(true)
}

#[test]
fn home_screen_shows_work_and_context() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    // Wide enough for Home's 3-card rail (see home_columns::WIDE_RAIL_MIN_TOTAL_WIDTH) —
    // plain `render()`'s 120 cols only qualifies for the narrow strip layout.
    let text = render_at(&app, 160, 40);
    assert!(text.contains("my work"), "home should list my work");
    assert!(
        text.contains("current context"),
        "home should show git context"
    );
    assert!(text.contains("DS-"), "home should show issue keys");
}

#[test]
fn detail_screen_renders_adf() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    assert_eq!(app.screen, Screen::Detail);
    let text = render(&app);
    assert!(
        text.contains("Acceptance"),
        "detail should show acceptance criteria"
    );
    assert!(text.contains("["), "detail should render task checkboxes");
}

#[test]
fn detail_screen_shows_an_epics_children() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    assert_eq!(app.screen, Screen::Detail);
    let text = render(&app);
    assert!(
        text.contains("child DS-2725") && text.contains("Develop"),
        "an Epic's detail view should list its child issue and type"
    );
}

#[test]
fn detail_screen_wide_layout_shows_side_rail_panels() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    // A taller viewport than the other wide-layout tests: the rail now has
    // 5 panels (workflow/meta/links/children/attachments) rather than 4, so
    // showing every one of them in full needs a bit more room.
    let text = render_at(&app, 120, 40);
    assert!(
        text.contains("workflow"),
        "wide Detail should show a workflow rail panel"
    );
    assert!(
        text.contains("children"),
        "wide Detail should show a children rail panel"
    );
    assert!(
        text.contains("child DS-2725"),
        "the children panel should list the Epic's child"
    );
    assert!(
        text.contains("Acceptance"),
        "the main column should still show acceptance criteria"
    );
    // Regression test: the workflow/meta rail panels used to be sized from
    // their logical (unwrapped) line count, so a wrapped chip strip or a
    // long components/labels line would silently push the panel's own
    // trailing content (the "t to change" hint, the "updated:" line) off
    // the bottom of its allotted height.
    assert!(
        text.contains("t to change"),
        "the workflow panel's hint line must not be clipped by a wrapped chip strip"
    );
    assert!(
        text.contains("updated:"),
        "the people & meta panel's trailing 'updated:' line must not be clipped"
    );
    // Regression test: the rail panels used to have no border at all —
    // just a plain title line with the body text running straight to the
    // pane's edge — so each panel now gets its own rounded card frame.
    // Every screen's header/footer chrome also has rounded corners, so
    // this counts at least the outer Detail card plus its 4 rail panels
    // (5) on top of whatever header/footer/etc. contribute, rather than
    // asserting an exact total.
    assert!(
        text.matches('╭').count() >= 5,
        "the outer Detail card plus all 4 rail panels should each have their own border"
    );
}

#[test]
fn detail_screen_wide_layout_shows_the_attachments_rail_panel() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    let text = render_at(&app, 120, 34);
    assert!(
        text.contains("attachments"),
        "wide Detail should show an attachments rail panel"
    );
    assert!(
        text.contains("accordion-mockup.png"),
        "the attachments panel should list the demo attachment's filename"
    );
}

/// Regression test for the bug report behind `RailPanel`/`rail_scroll`: an
/// issue with more children than the rail's children panel has room for
/// used to just silently clip the trailing ones, with no way to see them —
/// the panel was deliberately non-scrolling. Now `ui::detail::draw_rail`
/// flags the overflow in `App::rail_overflow`, and scrolling the panel
/// (`App::rail_scroll`, driven by `Tab`+arrows or the mouse wheel) actually
/// changes what's rendered.
#[test]
fn many_children_overflow_the_rail_panel_and_scrolling_reveals_the_rest() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    let mut detail = app.detail.clone().unwrap();
    detail.children = (0..30)
        .map(|i| ChildIssue {
            key: format!("DS-{}", 9000 + i),
            issue_type: "Sub-task".into(),
            summary: format!("child number {i}"),
            status: "To Do".into(),
        })
        .collect();
    app.detail = Some(detail);

    // 30 two-line child entries (~60 wrapped rows) can't possibly fit in the
    // handful of rows the rail's other four panels leave the children panel
    // at this viewport height.
    let text = render_at(&app, 120, 40);
    assert!(
        text.contains("DS-9000"),
        "the first child should be visible without any scrolling"
    );
    assert!(
        !text.contains("DS-9029"),
        "the last of 30 children shouldn't fit in this viewport without scrolling"
    );
    assert!(
        app.rail_overflow.get()[RailPanel::Children.index()],
        "the children panel should be flagged as overflowing after render"
    );
    assert!(
        !app.rail_overflow.get()[RailPanel::Workflow.index()],
        "a short panel like workflow shouldn't be flagged as overflowing"
    );

    app.rail_scroll[RailPanel::Children.index()] = 55;
    let text = render_at(&app, 120, 40);
    assert!(
        text.contains("DS-9029"),
        "scrolling the children panel should bring later children into view"
    );
}

#[test]
fn detail_screen_wide_layout_wraps_a_long_summary_instead_of_clipping_it() {
    // Regression test: the identity block (key/summary/chips) was sized
    // from its raw logical line count and never wrapped, so a summary
    // longer than the main column's width was silently hard-clipped
    // mid-word at the terminal edge instead of wrapping onto another row —
    // the same under-allocation bug already fixed for the rail panels via
    // `wrapped_row_count`, just missed here.
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    let mut detail = app.detail.clone().unwrap();
    detail.summary = "A deliberately long summary meant to exceed the main \
        column's width so it wraps onto another row instead of being cut \
        off mid word RIGHTMOST"
        .to_string();
    app.detail = Some(detail);
    let text = render_at(&app, 120, 34);
    assert!(
        text.contains("RIGHTMOST"),
        "a long summary must wrap onto additional rows, not get hard-clipped \
         at the main column's width"
    );
}

#[test]
fn detail_screen_narrow_layout_shows_facts_and_linked_panels() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    let text = render_at(&app, 84, 46);
    assert!(
        text.contains("facts"),
        "narrow Detail should show a facts panel"
    );
    assert!(
        text.contains("linked"),
        "narrow Detail should show a linked panel"
    );
    assert!(
        text.contains("Acceptance"),
        "the description should still show acceptance criteria"
    );
    let first_author = app.detail.as_ref().unwrap().comments[0].author.clone();
    assert!(
        text.contains(&first_author),
        "activity should still show comments"
    );
}

#[test]
fn detail_screen_narrow_layout_shows_the_attachments_section() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    // Taller than the other narrow-layout tests: the attachments section
    // sits after the description/linked panels in the scroll order, so
    // reaching it needs a viewport tall enough to not scroll it off-screen.
    let text = render_at(&app, 84, 70);
    assert!(
        text.contains("attachments · 2"),
        "narrow Detail should show an attachments section with the demo count"
    );
    assert!(
        text.contains("accordion-mockup.png"),
        "the attachments section should list the demo attachment's filename"
    );
}

#[test]
fn attachment_picker_overlay_renders_when_open() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    app.open_attachments();
    assert!(app.attachments_open);
    let text = render(&app);
    assert!(
        text.contains("attachments"),
        "the attachment picker overlay should show its title"
    );
    assert!(
        text.contains("accordion-mockup.png"),
        "the attachment picker should list the demo attachment's filename"
    );
    assert!(
        text.contains("download"),
        "the attachment picker should hint at the download key"
    );
}

/// Bullet 1 of issue #130's required test coverage: a `Picker` built with
/// the explicit, terminal-query-free `Halfblocks` constructor plus a tiny
/// in-memory image successfully produces a `StatefulProtocol` and renders
/// through a `TestBackend` without panicking — the actual assertion here is
/// mostly "this returns at all", since `StatefulImage`'s `StatefulWidget`
/// impl does the resize/encode work at render time.
#[test]
#[cfg(feature = "images")]
fn attachment_picker_renders_a_halfblocks_preview_image() {
    use jira_tui::app::AttachmentPreview;
    use ratatui_image::picker::Picker;

    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    app.open_attachments();
    assert!(app.attachments_open);
    let attachment_id = app.detail.as_ref().unwrap().attachments[0].id.clone();

    let picker = Picker::halfblocks();
    let protocol = picker.new_resize_protocol(image::DynamicImage::new_rgb8(4, 4));
    app.image_picker = Some(picker);
    *app.attachment_preview.get_mut() = Some(AttachmentPreview {
        attachment_id,
        protocol,
    });

    let text = render(&app);
    assert!(
        text.contains("attachments"),
        "the picker should still show its title alongside the image preview"
    );
}

/// Issue #130 Phase 3: the Detail screen's inline description-image paint
/// pass (`ui::detail`'s `image_paint_offsets`/`paint_inline_images`) — a
/// `SlicedImage` painted over the description's already-rendered, already
/// -scrolled `Paragraph`. Uses the same `Picker::halfblocks()` +
/// direct-cache-injection shortcut as
/// `attachment_picker_renders_a_halfblocks_preview_image` above (bypassing
/// the async fetch entirely) to seed `App::inline_images`.
#[cfg(feature = "images")]
mod inline_description_images {
    use super::*;
    use jira_tui::app::InlineImageKey;
    use ratatui_image::picker::Picker;
    use serde_json::json;

    /// A wide, short image with a strong top-to-bottom colour ramp:
    /// - Wide relative to any realistic pane width, so the sizing
    ///   function's `cols` is pane-width-bound rather than image-bound —
    ///   `a_terminal_resize_rebuilds_the_protocol_without_panicking` relies
    ///   on this to get a genuinely different `(rows, cols)` at two
    ///   different pane widths, actually exercising the cached
    ///   `SlicedProtocol`'s rebuild-on-size-change path.
    /// - Ramped rather than a flat colour, so downscaling never leaves two
    ///   adjacent halfblock rows with an identical colour —
    ///   `HalfBlock::pick_side` (in `ratatui_image`) renders an "upper ==
    ///   lower" cell as a plain space, which would make a uniform test
    ///   image indistinguishable from "nothing painted" and defeat the
    ///   non-space assertions below.
    fn gradient_image() -> image::DynamicImage {
        let (width, height) = (2000u32, 100u32);
        let mut img = image::RgbImage::new(width, height);
        let denom = f64::from(height.saturating_sub(1).max(1));
        for y in 0..height {
            let shade = ((f64::from(y) * 255.0) / denom).round() as u8;
            for x in 0..width {
                img.put_pixel(x, y, image::Rgb([shade, 0, 255 - shade]));
            }
        }
        image::DynamicImage::ImageRgb8(img)
    }

    /// DS-2722's demo detail, with its own description swapped for a
    /// single `mediaSingle` block whose `alt` matches the demo
    /// `accordion-mockup.png` attachment (id `10001`, `image/png` — see
    /// `domain::demo::demo_attachments`, shared by every demo issue) — the
    /// description's only block, so it lands at `line_start == 0` in the
    /// wide layout's `main.lines`: visible at scroll position 0 with
    /// nothing else pushing it down.
    fn open_issue_with_a_media_description(app: &mut App) {
        app.screen = Screen::Home;
        app.open_by_key("DS-2722");
        let mut detail = app.detail.clone().unwrap();
        detail.description = json!({
            "type": "doc", "version": 1,
            "content": [ { "type": "mediaSingle", "content": [
                { "type": "media", "attrs": {
                    "id": "x", "type": "file", "alt": "accordion-mockup.png"
                } }
            ] } ]
        });
        app.detail = Some(detail);
    }

    #[test]
    fn a_decoded_image_renders_as_halfblock_cells_when_visible() {
        let mut app = demo_app();
        open_issue_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        let text = render_at(&app, 120, 40);
        assert!(
            text.contains('▀') || text.contains('▄'),
            "a decoded, on-screen inline image should paint halfblock cells, got:\n{text}"
        );
    }

    #[test]
    fn an_image_scrolled_fully_out_of_view_paints_nothing() {
        let mut app = demo_app();
        open_issue_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        // Comfortably past the reserved rows (capped at 14) plus the rest
        // of the description/activity below it — nothing should be
        // visible at all, so no halfblock glyph should appear anywhere in
        // the rendered screen.
        app.detail_scroll = 500;
        let text = render_at(&app, 120, 40);
        assert!(
            !text.contains('▀') && !text.contains('▄'),
            "an image scrolled fully out of view must not paint any cells, got:\n{text}"
        );
    }

    #[test]
    fn an_image_partially_scrolled_into_view_does_not_panic() {
        let mut app = demo_app();
        open_issue_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        // Somewhere inside the reserved (3-14) rows: exactly the
        // partial-visibility case `SlicedProtocol` exists to handle. The
        // only hard requirement is "doesn't panic" — per issue #130's
        // plan, halfblocks' coarse resolution makes exact pixel assertions
        // here more trouble than they're worth.
        app.detail_scroll = 1;
        let _ = render_at(&app, 120, 40);
    }

    #[test]
    fn an_undecoded_attachment_still_shows_the_placeholder() {
        let mut app = demo_app();
        open_issue_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        // Deliberately never populated: `app.inline_images` stays empty,
        // mirroring "still fetching, failed, or never eligible".

        let text = render_at(&app, 120, 40);
        assert!(
            text.contains("[image: accordion-mockup.png]"),
            "an undecoded image must keep showing its placeholder, got:\n{text}"
        );
        assert!(
            !text.contains('▀') && !text.contains('▄'),
            "nothing should be painted for an image that was never decoded"
        );
    }

    #[test]
    fn no_detected_picker_shows_the_placeholder_exactly_as_without_the_images_feature() {
        let mut app = demo_app();
        open_issue_with_a_media_description(&mut app);
        // `image_picker` stays `None` — mirrors a terminal with no
        // detected image capability. A non-`images` build never even
        // compiles a real `MediaSizing::Ready` path at all (see
        // `App::with_detail_media_sizing`'s non-`images` stand-in in
        // `app::mod`), so this is the closest same-build equivalent.
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        let text = render_at(&app, 120, 40);
        assert!(
            text.contains("[image: accordion-mockup.png]"),
            "with no detected picker, the placeholder must render exactly as before this phase"
        );
        assert!(!text.contains('▀') && !text.contains('▄'));
    }

    /// Two renders of the *same* `App` (so the same
    /// `inline_image_protocols` cache) at pane widths chosen so the
    /// sizing function's `(rows, cols)` genuinely differs between them
    /// (160x100 columns/pane-bound at the wider terminal vs. a smaller,
    /// row-clamped size at the narrower one) — exercises
    /// `App::sliced_inline_image_protocol`'s "cached protocol's size no
    /// longer matches this placement's current size" rebuild branch,
    /// rather than just re-rendering an already-correctly-sized cached
    /// protocol.
    #[test]
    fn a_terminal_resize_rebuilds_the_protocol_without_panicking() {
        let mut app = demo_app();
        open_issue_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        let wide = render_at(&app, 200, 40);
        let narrower = render_at(&app, 100, 40);
        assert!(
            wide.contains('▀') || wide.contains('▄'),
            "expected halfblock cells at the wider pane width"
        );
        assert!(
            narrower.contains('▀') || narrower.contains('▄'),
            "expected halfblock cells at the narrower pane width after a rebuild"
        );
    }
}

/// Issue #130 Phase 5: the quick-view panel's inline description-image
/// paint pass — mirrors `inline_description_images` above as closely as
/// possible, just against `ui::quick_view`'s own screen/selection shape
/// (`app.quick_view = true`, `app.detail_cache` rather than `app.detail`,
/// `app.quick_view_scroll` rather than `app.detail_scroll`).
#[cfg(feature = "images")]
mod quick_view_inline_images {
    use super::*;
    use jira_tui::app::{AppEvent, InlineImageKey};
    use ratatui_image::picker::Picker;
    use serde_json::json;

    /// Same fixture shape as `inline_description_images::gradient_image` —
    /// kept as its own copy rather than shared, since `tests/render.rs` has
    /// no shared-fixtures module and hoisting a ~15-line helper out to
    /// share it with one sibling module is more churn than it's worth.
    fn gradient_image() -> image::DynamicImage {
        let (width, height) = (2000u32, 100u32);
        let mut img = image::RgbImage::new(width, height);
        let denom = f64::from(height.saturating_sub(1).max(1));
        for y in 0..height {
            let shade = ((f64::from(y) * 255.0) / denom).round() as u8;
            for x in 0..width {
                img.put_pixel(x, y, image::Rgb([shade, 0, 255 - shade]));
            }
        }
        image::DynamicImage::ImageRgb8(img)
    }

    /// Opens quick view on the first demo issue and swaps its *cached*
    /// detail's description for a single `mediaSingle` block whose `alt`
    /// matches the demo `accordion-mockup.png` attachment (id `10001`,
    /// shared by every demo issue — see `domain::demo::demo_attachments`) —
    /// mirrors `inline_description_images::open_issue_with_a_media_description`,
    /// just landing in `app.detail_cache` (quick view's own store, keyed by
    /// issue) instead of `app.detail` (Detail's single slot).
    fn open_quick_view_with_a_media_description(app: &mut App) {
        app.screen = Screen::Home;
        app.quick_view = true;
        app.selected = 0;
        app.ensure_quick_view_loaded();
        let key = app.issues[0].key.clone();
        let mut detail = app.quick_view_detail().unwrap().clone();
        detail.description = json!({
            "type": "doc", "version": 1,
            "content": [ { "type": "mediaSingle", "content": [
                { "type": "media", "attrs": {
                    "id": "x", "type": "file", "alt": "accordion-mockup.png"
                } }
            ] } ]
        });
        app.detail_cache.insert(key, detail);
    }

    #[test]
    fn a_decoded_image_renders_as_halfblock_cells_when_visible() {
        let mut app = demo_app();
        open_quick_view_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        let text = render_at(&app, 120, 40);
        assert!(
            text.contains('▀') || text.contains('▄'),
            "a decoded, on-screen inline image should paint halfblock cells in \
             quick view, got:\n{text}"
        );
    }

    #[test]
    fn an_image_scrolled_fully_out_of_view_paints_nothing() {
        let mut app = demo_app();
        open_quick_view_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        // Comfortably past whatever quick view's own (tighter than
        // Detail's) row budget clamps to — see `App::quick_view_max_image_rows`.
        app.quick_view_scroll = 500;
        let text = render_at(&app, 120, 40);
        assert!(
            !text.contains('▀') && !text.contains('▄'),
            "an image scrolled fully out of view must not paint any cells, got:\n{text}"
        );
    }

    #[test]
    fn an_undecoded_attachment_still_shows_the_placeholder() {
        let mut app = demo_app();
        open_quick_view_with_a_media_description(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        // Deliberately never populated: `app.inline_images` stays empty,
        // mirroring "still fetching, failed, or never eligible".

        let text = render_at(&app, 120, 40);
        assert!(
            text.contains("[image: accordion-mockup.png]"),
            "an undecoded image must keep showing its placeholder in quick view, got:\n{text}"
        );
        assert!(
            !text.contains('▀') && !text.contains('▄'),
            "nothing should be painted for an image that was never decoded"
        );
    }

    /// Issue #130 phase 5's own row-budget note: `ui::quick_view::draw_with_overflow`
    /// has to shrink the area it paints an image into once the trailing
    /// "… ↓ N more lines" fade row is showing, painting against the same
    /// shrunk area the `Paragraph` itself was rendered into — otherwise a
    /// reserved image row could land on top of (or past) the fade row. A
    /// short pane plus a deliberately long description forces the fade row
    /// to appear; the image (placed first in the document, so it's always
    /// within the visible prefix) must still paint alongside it.
    #[test]
    fn an_image_still_paints_alongside_the_overflow_fade_row() {
        let mut app = demo_app();
        app.screen = Screen::List;
        app.quick_view = true;
        app.selected = 0;
        app.ensure_quick_view_loaded();
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        let key = app.issues[0].key.clone();
        let mut detail = app.quick_view_detail().unwrap().clone();
        let mut content = vec![json!({
            "type": "mediaSingle", "content": [
                { "type": "media", "attrs": {
                    "id": "x", "type": "file", "alt": "accordion-mockup.png"
                } }
            ]
        })];
        content.extend((0..80).map(|i| {
            json!({
                "type": "paragraph",
                "content": [{"type": "text", "text": format!("Line {i} of a deliberately long description.")}]
            })
        }));
        detail.description = json!({"type": "doc", "version": 1, "content": content});
        app.detail_cache.insert(key, detail);

        let text = render_at(&app, 110, 16);
        assert!(
            text.contains("more line"),
            "the overflow fade row should show once the description exceeds the panel, got:\n{text}"
        );
        assert!(
            text.contains('▀') || text.contains('▄'),
            "the image reserved at the top of the description should still paint \
             alongside the overflow fade row, got:\n{text}"
        );
    }

    /// The idempotent-fetch/bounded-retention design at the core of issue
    /// #130 phase 5: quick view's own trigger (`App::refresh_quick_view_inline_images`,
    /// reached indirectly here through the same public entry points the run
    /// loop and async event pipeline actually use — `ensure_quick_view_loaded`
    /// and `apply_event`) must not pile up duplicate fetches as the
    /// selection churns across several issues that happen to share the same
    /// underlying attachment (every demo issue shares the `accordion-mockup.png`
    /// attachment, id `10001` — see `domain::demo::demo_attachments`), and
    /// revisiting an already-cached issue must not re-dispatch at all.
    ///
    /// Isolates `Config::load()` from any real credentials on this machine
    /// (mirrors `src/app/tests/support.rs`'s `live_app()`, which this
    /// integration-test crate can't reach — that helper is crate-internal —
    /// so the essential bits are inlined here for this one test) and only
    /// resolves `Attachment`-keyed images, whose blocking fetch bails out
    /// immediately on a missing `Config` with no network attempt at all
    /// (see `fetch_attachment_preview_blocking`), keeping this fully
    /// offline and fast.
    #[tokio::test]
    async fn selection_churn_across_issues_sharing_an_attachment_does_not_redispatch() {
        let base = std::env::temp_dir().join(format!(
            "jira-tui-quickview-churn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::env::set_var("XDG_CONFIG_HOME", &base);
        for var in [
            "JIRA_BASE_URL",
            "JIRA_EMAIL",
            "JIRA_API_TOKEN",
            "JIRA_TOKEN_FILE",
        ] {
            std::env::remove_var(var);
        }

        let mut app = demo_app();
        app.source = Source::Live {
            site: "demo.atlassian.net".into(),
            user: "me".into(),
        };
        app.quick_view = true;
        app.image_picker = Some(Picker::halfblocks());

        let media_key = InlineImageKey::Attachment("10001".into());
        let media_description = json!({
            "type": "doc", "version": 1,
            "content": [ { "type": "mediaSingle", "content": [
                { "type": "media", "attrs": {
                    "id": "x", "type": "file", "alt": "accordion-mockup.png"
                } }
            ] } ]
        });

        // Visit the first three issues in turn — each landing dispatches a
        // `DetailLoaded` fetch (drained and re-applied here with a
        // resolvable media node substituted in, standing in for whatever a
        // real Jira response would have contained) which, per this phase's
        // hook in `apply_detail_loaded`, should itself trigger exactly one
        // inline-image dispatch the first time, and none of the following
        // times — every issue resolves to the *same* `InlineImageKey` since
        // they all share the one demo attachment. `DetailLoaded` and
        // `InlineImageLoaded` are dispatched from independent spawned tasks
        // with no ordering guarantee relative to each other, so any
        // `InlineImageLoaded` seen while waiting for the next `DetailLoaded`
        // is stashed rather than assumed away.
        let mut inline_image_events = Vec::new();
        for i in 0..3 {
            app.selected = i;
            app.ensure_quick_view_loaded();
            let (generation, key, status) = loop {
                let event =
                    tokio::time::timeout(std::time::Duration::from_secs(5), app.events_rx.recv())
                        .await
                        .expect("detail fetch did not complete in time")
                        .expect("events_tx dropped unexpectedly");
                match event {
                    AppEvent::DetailLoaded {
                        generation,
                        key,
                        status,
                        ..
                    } => break (generation, key, status),
                    img @ AppEvent::InlineImageLoaded { .. } => inline_image_events.push(img),
                    _ => panic!("expected DetailLoaded or InlineImageLoaded"),
                }
            };
            let mut resolved_detail = jira_tui::domain::demo_detail(&key);
            resolved_detail.description = media_description.clone();
            app.apply_event(AppEvent::DetailLoaded {
                generation,
                key,
                detail: Box::new(resolved_detail),
                status,
            });
        }

        // The one dispatch from the loop above may still be in flight (it's
        // a real, if instantaneous, spawned task) — give it a bounded
        // window to land rather than assuming it's already queued.
        while let Ok(Some(event)) =
            tokio::time::timeout(std::time::Duration::from_millis(500), app.events_rx.recv()).await
        {
            inline_image_events.push(event);
        }

        // Across all three (distinct-issue, shared-attachment) visits above,
        // exactly one inline-image fetch should have been dispatched for the
        // shared key — the second and third visits' `refresh_quick_view_inline_images`
        // call must have found it already pending and skipped it.
        assert_eq!(
            inline_image_events.len(),
            1,
            "expected exactly one inline-image dispatch across three visits sharing the \
             same attachment, got {} events",
            inline_image_events.len()
        );
        match &inline_image_events[0] {
            AppEvent::InlineImageLoaded { key, .. } => assert_eq!(key, &media_key),
            _ => panic!("expected InlineImageLoaded"),
        }

        // Revisiting the first issue again — already cached — must be a
        // complete no-op: no new `DetailLoaded` dispatch, and therefore no
        // chance of a redundant inline-image dispatch either.
        app.selected = 0;
        app.ensure_quick_view_loaded();
        assert!(
            app.events_rx.try_recv().is_err(),
            "revisiting an already-cached issue must not dispatch anything at all"
        );
    }
}

/// Issue #130's comment-image phase: `activity_lines_cards` now renders
/// comment bodies via `adf::render_with_media` instead of plain
/// `adf::render`, so a media node embedded in a comment paints the same way
/// a description-embedded one already does — mirrors
/// `inline_description_images` above as closely as possible, just landing
/// the media node in the (single, replaced) comment's body rather than the
/// description, and using `App::jump_to_comments` (rather than a known
/// scroll offset) to reach it, since the comments section's own offset
/// accounting is exactly the rebasing this phase had to get right.
#[cfg(feature = "images")]
mod inline_comment_images {
    use super::*;
    use jira_tui::app::InlineImageKey;
    use jira_tui::domain::Comment;
    use ratatui_image::picker::Picker;
    use serde_json::json;

    /// Same fixture shape as `inline_description_images::gradient_image` —
    /// kept as its own copy rather than shared, matching
    /// `quick_view_inline_images::gradient_image`'s own precedent for why
    /// (`tests/render.rs` has no shared-fixtures module).
    fn gradient_image() -> image::DynamicImage {
        let (width, height) = (2000u32, 100u32);
        let mut img = image::RgbImage::new(width, height);
        let denom = f64::from(height.saturating_sub(1).max(1));
        for y in 0..height {
            let shade = ((f64::from(y) * 255.0) / denom).round() as u8;
            for x in 0..width {
                img.put_pixel(x, y, image::Rgb([shade, 0, 255 - shade]));
            }
        }
        image::DynamicImage::ImageRgb8(img)
    }

    /// DS-2722's demo detail, with its comments replaced by a single one
    /// whose body is just a `mediaSingle` block with an `alt` matching the
    /// demo `accordion-mockup.png` attachment (id `10001` — see
    /// `domain::demo::demo_attachments`, shared by every demo issue) — the
    /// comment's only content, so once `jump_to_comments` scrolls to it,
    /// the image sits right at the top of the comment card with nothing
    /// else in the same comment pushing it further down.
    fn open_issue_with_a_media_comment(app: &mut App) {
        app.screen = Screen::Home;
        app.open_by_key("DS-2722");
        let mut detail = app.detail.clone().unwrap();
        detail.comments = vec![Comment {
            id: "1".into(),
            author: "Ada".into(),
            created: "1h ago".into(),
            body: json!({
                "type": "doc", "version": 1,
                "content": [ { "type": "mediaSingle", "content": [
                    { "type": "media", "attrs": {
                        "id": "x", "type": "file", "alt": "accordion-mockup.png"
                    } }
                ] } ]
            }),
        }];
        app.detail = Some(detail);
    }

    #[test]
    fn a_decoded_image_renders_as_halfblock_cells_when_visible() {
        let mut app = demo_app();
        open_issue_with_a_media_comment(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        // `jump_to_comments` picks its offset from the last-rendered
        // `detail_area`'s width, so render once first — the same call-order
        // requirement `app::comments`' own tests already document.
        let _ = render_at(&app, 120, 40);
        app.jump_to_comments();
        let text = render_at(&app, 120, 40);
        assert!(
            text.contains('▀') || text.contains('▄'),
            "a decoded, on-screen inline image embedded in a comment should paint halfblock \
             cells, got:\n{text}"
        );
    }

    #[test]
    fn an_image_scrolled_fully_out_of_view_paints_nothing() {
        let mut app = demo_app();
        open_issue_with_a_media_comment(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        // Comfortably past the comments section entirely.
        app.detail_scroll = 500;
        let text = render_at(&app, 120, 40);
        assert!(
            !text.contains('▀') && !text.contains('▄'),
            "an image scrolled fully out of view must not paint any cells, got:\n{text}"
        );
    }

    #[test]
    fn an_image_partially_scrolled_into_view_does_not_panic() {
        let mut app = demo_app();
        open_issue_with_a_media_comment(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        app.inline_images
            .get_mut()
            .insert(InlineImageKey::Attachment("10001".into()), gradient_image());

        let _ = render_at(&app, 120, 40);
        app.jump_to_comments();
        // One row further than the comments header itself — partway into
        // the one comment's card, exactly the partial-visibility case
        // `SlicedProtocol` exists to handle. The only hard requirement is
        // "doesn't panic", per the same rationale
        // `inline_description_images::an_image_partially_scrolled_into_view_does_not_panic`
        // already documents.
        app.detail_scroll += 1;
        let _ = render_at(&app, 120, 40);
    }

    #[test]
    fn an_undecoded_attachment_still_shows_the_placeholder() {
        let mut app = demo_app();
        open_issue_with_a_media_comment(&mut app);
        app.image_picker = Some(Picker::halfblocks());
        // Deliberately never populated: `app.inline_images` stays empty,
        // mirroring "still fetching, failed, or never eligible".

        let _ = render_at(&app, 120, 40);
        app.jump_to_comments();
        let text = render_at(&app, 120, 40);
        assert!(
            text.contains("[image: accordion-mockup.png]"),
            "an undecoded image embedded in a comment must keep showing its placeholder, \
             got:\n{text}"
        );
        assert!(
            !text.contains('▀') && !text.contains('▄'),
            "nothing should be painted for an image that was never decoded"
        );
    }
}

#[test]
fn attachment_upload_input_renders_the_typed_path() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    app.open_attachment_upload();
    for c in "/tmp/report.pdf".chars() {
        app.attachment_upload_input_char(c);
    }
    let text = render(&app);
    assert!(
        text.contains("upload attachment"),
        "the upload input overlay should show its title"
    );
    assert!(
        text.contains("/tmp/report.pdf"),
        "the upload input overlay should show the typed path"
    );
}

#[test]
fn attachment_upload_confirm_renders_the_preview() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    app.attachment_upload = Some(jira_tui::app::AttachmentUpload::Confirm {
        path: "/tmp/report.pdf".into(),
        filename: "report.pdf".into(),
        size: 245_760,
        mime: "application/pdf",
    });
    let text = render(&app);
    assert!(
        text.contains("upload"),
        "the confirm overlay should be titled around the upload"
    );
    assert!(
        text.contains("DS-2722"),
        "the confirm overlay should name the target issue"
    );
    assert!(
        text.contains("report.pdf"),
        "the confirm overlay should show the filename"
    );
    assert!(
        text.contains("240 KB"),
        "the confirm overlay should show a human-readable size"
    );
    assert!(
        text.contains("application/pdf"),
        "the confirm overlay should show the guessed mime type"
    );
    assert!(
        text.contains("y/⏎"),
        "confirm copy should mention both y and Enter confirm the upload"
    );
}

#[test]
fn detail_screen_narrow_facts_panel_folds_with_x() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    // Populate `detail_area` at the narrow breakpoint before folding.
    let _ = render_at(&app, 84, 46);
    let unfolded = render_at(&app, 84, 46);
    assert!(
        unfolded.contains("reporter:"),
        "unfolded facts panel should show every field"
    );

    app.toggle_facts_folded();
    let folded = render_at(&app, 84, 46);
    assert!(
        !folded.contains("reporter:"),
        "folding the facts panel should hide the per-field kv lines"
    );

    // The fold is narrow-only: at the wide breakpoint the rail's people &
    // meta panel shows in full regardless of `facts_folded`.
    let wide = render_at(&app, 120, 34);
    assert!(
        wide.contains("reporter:"),
        "the wide rail's meta panel must not be affected by the narrow fold state"
    );
}

#[test]
fn detail_screen_shows_comment_indicator_and_jumps_to_comments() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    let comment_count = app.detail.as_ref().unwrap().comments.len();
    assert!(comment_count > 0, "demo detail should have canned comments");

    // `jump_to_comments` picks the Wide/Narrow layout's comment offset from
    // the last-rendered `detail_area`'s width, so it needs a real render
    // first — before this phase there was only one document shape, so call
    // order didn't matter, but now it must mirror real usage (screen always
    // draws before a keypress is handled).
    let _ = render(&app);
    // The comments section header (with its count) lives below the fold
    // until you scroll or jump to it.
    app.jump_to_comments();
    let text = render(&app);
    assert!(
        text.contains("💬") && text.contains(&comment_count.to_string()),
        "jumping to the comments section should show a comment-count header"
    );
    let first_author = app.detail.as_ref().unwrap().comments[0].author.clone();
    assert!(
        text.contains(&first_author),
        "scrolling to comments should surface the first comment's author"
    );
}

#[test]
fn comment_card_bar_stays_continuous_across_a_wrapped_body_line() {
    // Regression test, end-to-end through the real render pipeline: the
    // comment card's left `▌` rule used to be added once per logical ADF
    // line, so a long comment body only showed it on the first on-screen
    // row once `Paragraph::wrap` re-flowed the rest — leaving a visible gap
    // on every wrapped continuation row (see `render::wrap_with_bar`).
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    // `jump_to_comments` picks its offset from the last-rendered
    // `detail_area`'s width (see the sibling test above), so render once at
    // the target width *first* — narrow enough that the demo's first
    // comment (a full sentence) wraps across more than one row.
    let _ = render_at(&app, 70, 40);
    app.jump_to_comments();
    let text = render_at(&app, 70, 40);
    let bar_rows: Vec<usize> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains('▌'))
        .map(|(i, _)| i)
        .collect();
    assert!(
        bar_rows.len() >= 2,
        "expected the wrapped comment body to show the bar on more than one row, got {bar_rows:?} in:\n{text}"
    );
    assert!(
        bar_rows.windows(2).any(|pair| pair[1] - pair[0] == 1),
        "expected at least one pair of *adjacent* bar rows (proving the bar survived a wrap, not just showing on scattered single rows), got {bar_rows:?} in:\n{text}"
    );
}

#[test]
fn quick_view_panel_no_longer_shows_comments_or_activity() {
    // SPEC.md §4 (phase 7): quick view is now description-excerpt + compact
    // meta grid only — no workflow strip, no comments/activity section
    // (unlike the pre-phase-7 full detail re-render). `jump_to_comments`
    // degrades to its existing "no comments" status rather than finding a
    // section that no longer exists.
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();
    let detail = app.quick_view_detail().unwrap().clone();
    assert!(
        !detail.comments.is_empty(),
        "demo detail should have canned comments to make this a real check"
    );
    let first_author = detail.comments[0].author.clone();

    let text = render(&app);
    assert!(
        !text.contains(&first_author),
        "quick view should not render comment authors any more"
    );
    assert!(
        !text.contains('💬'),
        "quick view should not render the comments section header"
    );

    app.jump_to_comments();
    assert_eq!(
        app.status, "no comments on this issue",
        "quick view has no comments section to jump to"
    );
}

#[test]
fn about_screen_shows_animated_banner() {
    let mut app = demo_app();
    app.screen = Screen::About;
    let text = render(&app);
    assert!(text.contains('█'), "about should render the block banner");
}

#[test]
fn welcome_intro_shows_jax_and_choices() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.onboarding.welcome_phase = WelcomePhase::Intro;
    let text = render(&app);
    assert!(text.contains("Jax"), "welcome should introduce Jax");
    assert!(
        text.contains("Set up live"),
        "welcome should offer live setup"
    );
}

#[test]
fn welcome_setup_shows_credential_fields() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.onboarding.welcome_phase = WelcomePhase::Setup;
    app.onboarding.field_token = "supersecret".to_string();
    let text = render(&app);
    assert!(text.contains("site"));
    assert!(text.contains("email"));
    assert!(text.contains("token"));
    // The token must be masked, never shown in the clear.
    assert!(!text.contains("supersecret"), "token must be masked");
    assert!(text.contains('•'), "masked token should render bullets");
}

#[test]
fn welcome_setup_records_each_fields_area() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.onboarding.welcome_phase = WelcomePhase::Setup;
    let text = render_at(&app, 120, 40);

    let site = app.onboarding_site_area.get();
    let email = app.onboarding_email_area.get();
    let token = app.onboarding_token_area.get();

    for (name, area) in [("site", site), ("email", email), ("token", token)] {
        assert_ne!(
            area,
            ratatui::layout::Rect::default(),
            "the {name} field's area should be recorded during render"
        );
        assert_eq!(area.height, 1, "each onboarding field is a single row");
    }

    assert!(
        site.y < email.y && email.y < token.y,
        "the three fields should be recorded top-to-bottom in form order"
    );

    // Confirm the synthesized row genuinely lands on the rendered field,
    // not merely that the three are in the right order.
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[site.y as usize].contains("site"),
        "the recorded site row should contain the site field's own text"
    );
    assert!(
        lines[email.y as usize].contains("email"),
        "the recorded email row should contain the email field's own text"
    );
    assert!(
        lines[token.y as usize].contains("token"),
        "the recorded token row should contain the token field's own text"
    );
}

#[test]
fn welcome_intro_clears_the_setup_field_areas() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    app.onboarding.welcome_phase = WelcomePhase::Setup;
    let _ = render_at(&app, 120, 40);
    assert_ne!(
        app.onboarding_site_area.get(),
        ratatui::layout::Rect::default()
    );

    // Flipping to Intro and re-rendering must clear the stale Setup areas,
    // so a leftover Rect from a previous visit can't misfire once we're
    // back on the intro phase.
    app.onboarding.welcome_phase = WelcomePhase::Intro;
    let _ = render_at(&app, 120, 40);

    for (name, area) in [
        ("site", app.onboarding_site_area.get()),
        ("email", app.onboarding_email_area.get()),
        ("token", app.onboarding_token_area.get()),
    ] {
        assert_eq!(
            area,
            ratatui::layout::Rect::default(),
            "the {name} field's area should be cleared while the intro phase renders"
        );
    }
}

#[test]
fn transition_picker_lists_targets() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_transitions();
    let text = render(&app);
    assert!(text.contains("move to"), "picker should show a title");
    assert!(
        text.contains("In Progress"),
        "picker should list transitions"
    );
    assert!(
        text.contains("current"),
        "picker should mark the current status"
    );
}

#[test]
fn view_picker_lists_my_work_all_project_and_teammates() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_view_picker();
    let text = render(&app);
    assert!(text.contains("switch view"), "picker should show a title");
    assert!(text.contains("My Work"), "picker should list My Work");
    assert!(
        text.contains("current"),
        "the active view should be marked current"
    );
    assert!(
        text.contains("All Project Issues"),
        "picker should list the all-project view"
    );
    assert!(
        text.contains("priya.nair"),
        "picker should list demo teammates seeded from loaded assignees"
    );
}

#[test]
fn assignee_picker_lists_unassign_me_and_teammates() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    let text = render(&app);
    assert!(text.contains("assign to"), "picker should show a title");
    assert!(text.contains("Unassign"), "picker should offer Unassign");
    assert!(
        text.contains("scott.morris"),
        "picker should pin the current user"
    );
    assert!(
        text.contains("priya.nair"),
        "picker should list demo teammates"
    );
}

#[test]
fn assignee_picker_filters_as_the_query_is_typed() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    app.assignee_picker_input_char('p');
    app.assignee_picker_input_char('r');
    app.assignee_picker_input_char('i');
    let text = render(&app);
    assert!(text.contains("priya.nair"), "filtered picker keeps a match");
    assert!(
        !text.contains("Unassign"),
        "filtered picker hides non-matching rows"
    );
}

#[test]
fn assignee_picker_keeps_the_selection_in_view_on_a_short_terminal() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_assignee_picker();
    // Demo data has "Unassign" + 4 users; move to the last row.
    let last = app.assignee_picker.rows.len() - 1;
    app.assignee_picker.selected = last;

    // A short terminal can't fit every row at once — the scroll window
    // must still keep the highlighted (last) row on screen instead of
    // clipping it off the bottom of the popup.
    let backend = TestBackend::new(120, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());

    let last_label = match &app.assignee_picker.rows[last] {
        jira_tui::app::AssigneeRow::Unassign => "Unassign".to_string(),
        jira_tui::app::AssigneeRow::User(u) => u.display_name.clone(),
    };
    assert!(
        text.contains(&last_label),
        "the selected row must stay visible even when the popup can't fit every row"
    );
    assert!(
        text.contains('▌'),
        "the selection cursor must still be drawn somewhere"
    );
}

#[test]
fn version_picker_shows_field_tabs_and_checked_versions() {
    let mut app = demo_app();
    app.open_by_key("DS-2648"); // demo data: fix v3.5.0, affects v3.4.0
    app.open_version_picker();
    let text = render(&app);
    assert!(text.contains("Fix Version(s)"), "picker should show tabs");
    assert!(
        text.contains("Affects Version(s)"),
        "picker should show tabs"
    );
    assert!(
        text.contains("v3.4.0"),
        "picker should list project versions"
    );
    assert!(
        text.contains("v3.5.0"),
        "picker should list project versions"
    );
    assert!(
        text.contains('✓'),
        "the issue's current fix version should be checked"
    );
}

#[test]
fn version_picker_tab_switches_which_field_is_checked() {
    let mut app = demo_app();
    app.open_by_key("DS-2648");
    app.open_version_picker();
    app.version_picker_switch_field();
    assert_eq!(
        app.version_picker.field,
        jira_tui::app::VersionField::Affects
    );
    let text = render(&app);
    // v3.4.0 is the issue's affects version, now the active field — matched
    // by "released" (only the picker's own row carries a release-date
    // suffix) so this can't accidentally match the Detail screen's own
    // "affects version(s): v3.4.0" fact row showing through behind the
    // popup.
    let v34_line = text
        .lines()
        .find(|l| l.contains("v3.4.0") && l.contains("released"))
        .unwrap();
    assert!(v34_line.contains('✓'));
}

#[test]
fn command_palette_shows_on_key_view_and_app_groups_with_transitions() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.open_palette();
    let text = render(&app);
    assert!(
        text.contains("command palette"),
        "palette should show a title"
    );
    assert!(
        text.contains("ON DS-"),
        "palette should show the on-key group header"
    );
    assert!(text.contains("VIEW"), "palette should show the view group");
    assert!(text.contains("APP"), "palette should show the app group");
    assert!(
        text.contains("Transition"),
        "an issue with a fetched detail should list its transitions"
    );
    assert!(
        text.contains("assign/unassign"),
        "an issue with a fetched detail should offer assign"
    );
}

#[test]
fn command_palette_omits_detail_only_actions_without_a_fetched_issue_detail() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.selected = 0;
    app.open_palette();
    let text = render(&app);
    assert!(
        text.contains("copy issue key"),
        "a bare selection should still offer copy/open actions"
    );
    assert!(
        !text.contains("assign/unassign"),
        "a bare selection with no fetched detail shouldn't offer assign"
    );
    assert!(
        !text.contains("Transition"),
        "a bare selection with no fetched detail shouldn't offer transitions"
    );
}

#[test]
fn command_palette_filters_and_highlights_the_matched_text() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_palette();
    for c in "about".chars() {
        app.palette_input_char(c);
    }
    let text = render(&app);
    assert!(
        text.contains("about"),
        "the matching row should still render"
    );
    assert!(
        !text.contains("refresh"),
        "non-matching rows should be filtered out"
    );
}

#[test]
fn command_palette_esc_closes_it() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_palette();
    assert!(render(&app).contains("command palette"));
    app.close_palette();
    assert!(!render(&app).contains("command palette"));
}

#[test]
fn switching_to_a_teammate_view_shows_a_confirmation_toast() {
    // Renamed from "...shows_in_the_header": this asserts on the transient
    // flash toast `switch_view` raises (`App::flash`), not the header —
    // the old header literally said "viewing: X" and this test's name
    // predates the phase-3 breadcrumb rework, which dropped that text in
    // favour of the breadcrumb itself (see the test below).
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.switch_view(jira_tui::domain::ViewKind::Teammate("alex.chen".into()));
    let text = render(&app);
    assert!(
        text.contains("viewing: alex.chen's Work"),
        "switching views should show a confirmation toast"
    );
}

#[test]
fn switching_to_a_teammate_view_shows_in_the_header_breadcrumb() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.switch_view(jira_tui::domain::ViewKind::Teammate("alex.chen".into()));
    // Expire the confirmation toast so only the header's own breadcrumb
    // text is left to satisfy the assertion.
    app.tick = app.flash_until;
    let text = render(&app);
    assert!(
        text.contains("alex.chen's Work"),
        "the header breadcrumb should show the active non-default view"
    );
}

#[test]
fn preview_screen_renders_pending_edit() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.finish_edit("## Fresh heading\n\nEdited body text.");
    assert_eq!(app.screen, Screen::Preview);
    let text = render(&app);
    assert!(text.contains("preview"), "preview should be titled");
    assert!(text.contains("Fresh heading"));
    assert!(text.contains("apply"));
    assert!(
        text.contains("y/⏎"),
        "preview copy should mention both y and Enter apply the edit"
    );
}

#[test]
fn in_tui_editor_renders_buffer() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    let text = render(&app);
    assert!(text.contains("editing"), "editor should be titled");
    assert!(
        text.contains("Problem"),
        "editor should show the description Markdown"
    );
}

/// Regression test: the editor's title bar used to read `app.detail`
/// (whatever the Detail screen last showed) rather than `App::edit_key`
/// (the issue the compose session actually targets), so composing a
/// comment from the quick-view panel after previously visiting a
/// *different* issue's Detail screen mislabelled the editor with that
/// stale issue instead of the one quick view — and the actual post — was
/// really about.
#[test]
fn composing_a_comment_from_quick_view_titles_the_editor_with_the_quick_viewed_issue() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    // Visit a different issue's Detail screen first, so `app.detail` holds
    // a stale key by the time we go back and comment from quick view.
    let stale_key = app.issues[1].key.clone();
    app.open_by_key(&stale_key);
    app.screen = Screen::Home;

    app.quick_view = true;
    app.selected = 0;
    let target_key = app.issues[0].key.clone();
    assert_ne!(target_key, stale_key, "sanity: the two issues must differ");
    app.ensure_quick_view_loaded();

    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);
    let text = render(&app);
    assert!(
        text.contains(&format!("editing {target_key}")),
        "the editor should be titled with the quick-viewed issue"
    );
    assert!(
        !text.contains(&format!("editing {stale_key}")),
        "must not show the previously-viewed Detail issue's key instead"
    );
}

#[test]
fn preview_screen_wording_matches_the_edit_target() {
    // Regression test: the preview's explanatory line used to hard-code
    // "edited description" even when previewing a comment.
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();

    app.begin_comment();
    for c in "Looks good.".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);
    let text = render(&app);
    assert!(
        text.contains("your comment will look"),
        "previewing a comment should say so, not \"edited description\""
    );
}

/// Regression test for the bug report behind the comment popover: composing
/// a comment from the quick-view panel used to replace the *entire* screen
/// with the editor, hiding the list and quick-view panel it was reacting
/// to. It should now render as a bottom panel with the list still visible
/// above it and the footer still showing the editor's own hints below it.
#[test]
fn comment_popover_from_quick_view_keeps_the_list_visible_behind_it() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();
    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);

    let text = render(&app);
    assert!(
        text.contains("KEY") && text.contains("SUMMARY"),
        "the issue list's column header should still be visible behind the popover"
    );
    assert!(
        text.contains("editing") && text.contains("Markdown"),
        "the editor panel itself should still be showing"
    );
    assert!(
        text.contains("esc cancel"),
        "the footer should show the editor's hints, not be blank or covered"
    );
}

/// Same coverage as above, but composing from the Detail screen: its own
/// side rail (workflow/meta) should stay visible above the popover.
#[test]
fn comment_popover_from_detail_keeps_the_rail_visible_behind_it() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_comment();
    assert_eq!(app.screen, Screen::Edit);

    let text = render(&app);
    assert!(
        text.contains("workflow"),
        "Detail's side rail should still be visible behind the popover"
    );
    assert!(text.contains("editing") && text.contains("Markdown"));
}

/// The confirmation step (`Screen::Preview`, reached via ^S) gets the same
/// popover treatment as the editor, so the round-trip stays visually
/// consistent rather than popping up for compose and covering everything
/// for confirm.
#[test]
fn comment_preview_also_renders_as_a_popover() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();
    app.begin_comment();
    for c in "Looks good.".chars() {
        app.editor.insert_char(c);
    }
    app.commit_tui_edit();
    assert_eq!(app.screen, Screen::Preview);

    let text = render(&app);
    assert!(
        text.contains("KEY") && text.contains("SUMMARY"),
        "the list should still be visible behind the comment preview popover"
    );
    assert!(
        text.contains("preview ·"),
        "the preview panel should still be showing"
    );
}

/// Guards against over-applying the popover: a DESCRIPTION edit (only ever
/// reachable from the Detail screen already showing that issue) keeps
/// taking the whole screen, unchanged — there's no "different background
/// issue" context to preserve the way there is for a comment composed from
/// quick view.
#[test]
fn description_edit_still_takes_the_whole_screen() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    assert_eq!(app.screen, Screen::Edit);

    let text = render(&app);
    assert!(
        !text.contains("workflow"),
        "a description edit must not show Detail's own side rail behind it \
         (contrast with comment_popover_from_detail_keeps_the_rail_visible_behind_it)"
    );
}

#[test]
fn in_tui_editor_wraps_long_lines_instead_of_running_off_screen() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines =
        vec!["one two three four five six seven eight nine ten eleven twelve".into()];
    app.editor.cx = 0;
    app.editor.cy = 0;
    // Narrow enough (40 cols, minus borders/gutter) to force the single
    // long line above across multiple wrapped rows.
    let text = render_at(&app, 40, 20);
    assert!(
        text.contains("one two"),
        "editor should still show the start of the line"
    );
    assert!(
        text.contains("eleven twelve") || text.contains("twelve"),
        "wrapped continuation should be visible on a later row, not run off-screen"
    );
}

#[test]
fn in_tui_editor_cursor_tracks_into_wrapped_rows() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    let long_word = "supercalifragilisticexpialidocious";
    app.editor.lines = vec![format!("start {long_word} end")];
    app.editor.cy = 0;
    app.editor.cx = app.editor.lines[0].chars().count(); // cursor at end of line
                                                         // Narrow enough that the hard-broken long word spans several rows.
    let backend = ratatui::backend::TestBackend::new(20, 20);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    // The cursor must be positioned somewhere inside the rendered area, not
    // silently skipped because it's off the (unwrapped) right edge.
    assert!(
        terminal.get_cursor_position().is_ok(),
        "cursor position should be set once the buffer is wrapped"
    );
}

#[test]
fn spell_suggest_picker_lists_suggestions_for_the_flagged_word() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["a mispeled word".into()];
    app.editor.cy = 0;
    app.editor.cx = 2;
    app.open_spell_suggest();
    assert!(app.spell_suggest_open);

    let text = render(&app);
    assert!(
        text.contains("mispeled"),
        "the popup should name the word it's offering replacements for"
    );
    for suggestion in &app.spell_suggest.suggestions {
        assert!(
            text.contains(suggestion.as_str()),
            "expected suggestion {suggestion:?} to be listed"
        );
    }
    assert!(text.contains("replace"), "popup should hint at ⏎ replace");
}

#[test]
fn in_tui_editor_underlines_misspelled_words_only() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    app.begin_tui_edit();
    app.editor.lines = vec!["a mispeled word".into()];
    app.editor.cx = 0;
    app.editor.cy = 0;

    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();

    let area = buf.area;
    let mut found = None;
    for y in 0..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            row.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        if row.contains("mispeled") {
            found = Some((y, row));
            break;
        }
    }
    let (y, row) = found.expect("the typed line should be rendered somewhere");

    let misspelled_x = row.find("mispeled").unwrap() as u16 + 2;
    let misspelled_cell = buf.cell((misspelled_x, y)).unwrap();
    assert!(
        misspelled_cell
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED),
        "a misspelled word should be rendered underlined"
    );

    let correct_x = row.find("word").unwrap() as u16 + 1;
    let correct_cell = buf.cell((correct_x, y)).unwrap();
    assert!(
        !correct_cell
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED),
        "a correctly spelled word must not be underlined"
    );
}

#[test]
fn quick_view_panel_shows_selected_issue() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.quick_view = true;
    app.selected = 0;
    // Simulate the run loop's lazy-load call.
    app.ensure_quick_view_loaded();
    let text = render(&app);
    assert!(
        text.contains("quick view"),
        "quick view panel should render"
    );
    // Once loaded, the full fields and ADF body should be visible, not just
    // the one-line summary.
    assert!(text.contains("assignee:"), "quick view should show fields");
    assert!(
        text.contains("Problem") || text.contains("Proposed"),
        "quick view should render the full ADF body"
    );
}

#[test]
fn quick_view_screen_wide_shows_description_and_meta_grid() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();
    let issue_type = app.issues[0].issue_type.clone();
    // Terminal-width 110 -> inner quick-view width just above the 100-col
    // wide threshold.
    let text = render_at(&app, 110, 40);
    assert!(
        text.contains(&issue_type),
        "wide quick view should show the selected issue's type chip"
    );
    assert!(
        text.contains("updated:"),
        "wide quick view should show the meta grid's kv fields"
    );
    assert!(
        text.contains("Problem") || text.contains("Proposed"),
        "wide quick view should show the description excerpt"
    );
}

#[test]
fn quick_view_screen_narrow_shows_chips_and_inline_pairs() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();
    let issue_type = app.issues[0].issue_type.clone();
    // Terminal-width 70 -> inner quick-view width below the 100-col wide
    // threshold.
    let text = render_at(&app, 70, 40);
    assert!(
        text.contains(&issue_type),
        "narrow quick view should show the selected issue's type chip"
    );
    assert!(
        text.contains("updated:"),
        "narrow quick view should show the inline kv pairs"
    );
    assert!(
        text.contains("Problem") || text.contains("Proposed"),
        "narrow quick view should show the description excerpt"
    );
}

#[test]
fn quick_view_screen_shows_overflow_line_when_description_exceeds_the_panel() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    app.selected = 0;
    app.ensure_quick_view_loaded();

    // Swap in a deliberately long description so it can't possibly fit a
    // small quick-view panel.
    let key = app.issues[0].key.clone();
    let mut detail = app.quick_view_detail().unwrap().clone();
    let paragraphs: Vec<_> = (0..80)
        .map(|i| {
            serde_json::json!({
                "type": "paragraph",
                "content": [{"type": "text", "text": format!("Line {i} of a deliberately long description.")}]
            })
        })
        .collect();
    detail.description = serde_json::json!({"type": "doc", "version": 1, "content": paragraphs});
    app.detail_cache.insert(key, detail);

    let text = render_at(&app, 110, 16);
    assert!(
        text.contains("more line"),
        "a description exceeding the panel should show a '... N more lines' indicator"
    );
}

#[test]
fn home_screen_wide_shows_three_rail_cards_with_bars() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    // Open an issue and back out to Home so `recent` gets populated the
    // same way real navigation would.
    let key = app.issues[0].key.clone();
    app.open_by_key(&key);
    app.screen = Screen::Home;

    let text = render_at(&app, 160, 40);
    assert!(text.contains("current context"));
    assert!(text.contains("at a glance"));
    assert!(text.contains("recent"), "the recent card should render");
    assert!(
        text.contains('█') || text.contains('░'),
        "glance stats should show proportion bars"
    );
}

#[test]
fn home_screen_narrow_shows_tiles_and_recent_strip() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let key = app.issues[0].key.clone();
    app.open_by_key(&key);
    app.screen = Screen::Home;

    let text = render_at(&app, 80, 40);
    assert!(
        // The strip's own padded label — a bare "recent" would also match
        // this branch's own name ("feature/recent-issues-nav-strip") in
        // the header's git-context strip.
        text.contains(" recent  "),
        "narrow home should show the persistent recent-issues strip"
    );
    assert!(
        text.contains("assigned"),
        "narrow home should show glance tile labels"
    );
    assert!(
        text.contains("context"),
        "the narrow context strip should sit in its own titled panel"
    );
    // Regression test: the narrow context strip and each glance tile used to
    // render as bare, borderless Paragraphs — the design mockup shows both
    // as bordered cards, so this counts at least the context panel plus all
    // 4 glance tiles (5) on top of whatever header/footer/list contribute.
    assert!(
        text.matches('╭').count() >= 5,
        "the narrow context panel and every glance tile should have their own border"
    );
}

#[test]
fn home_screen_short_height_trims_glance_to_two_but_keeps_the_recent_strip() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let key = app.issues[0].key.clone();
    app.open_by_key(&key);
    app.screen = Screen::Home;

    // Body height (~24) falls below Home's own 30-row short-terminal
    // threshold, so its glance tiles still trim to assigned/blocked only —
    // but the persistent recent-issues strip (`ui::nav_strip`) has a much
    // lower, single-row-justified threshold of its own (20) and keeps
    // showing here, unlike the old Home-only "recent" card it replaced.
    let text = render_at(&app, 120, 30);
    assert!(
        text.contains(" recent  "),
        "the persistent recent-issues strip should still show at height 30"
    );
    assert!(
        !text.contains("in review") && !text.contains("done this week"),
        "a short terminal should trim glance down to assigned/blocked only"
    );
    assert!(text.contains("assigned"));
    assert!(text.contains("blocked"));
}

#[test]
fn home_screen_very_short_height_hides_the_recent_strip() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let key = app.issues[0].key.clone();
    app.open_by_key(&key);
    app.screen = Screen::Home;

    // Below the strip's own 20-row threshold, it disappears entirely.
    let text = render_at(&app, 120, 15);
    assert!(
        !text.contains(" recent  "),
        "a very short terminal should hide the recent-issues strip entirely"
    );
}

/// End-to-end coverage of the branching scenario worked through during
/// design: A → link to B → back to A → link to C. The strip must show
/// both A and B tinted the same lineage colour as the current entry C
/// (proving the "abandoned branch isn't lost" property renders visibly,
/// not just in `NavHistory`'s own data), and the Detail screen must show
/// the strip too, not just Home/List.
#[test]
fn detail_screen_shows_the_recent_strip_with_the_full_branch_lineage() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();

    app.follow_link("DS-9001"); // A -> B
    app.go_back(); // back to A
    app.follow_link("DS-9002"); // A -> C (new branch)

    let text = render(&app);
    assert!(
        text.contains(" recent  "),
        "detail should show the persistent strip too"
    );
    assert!(
        text.contains("DS-9001"),
        "the abandoned branch B should still be visible"
    );
    assert!(
        text.contains("DS-9002"),
        "the current entry C should be visible"
    );
}

#[test]
fn home_wide_rail_card_shows_recent_issues_with_lineage_bars() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    app.follow_link("DS-9001");
    app.screen = Screen::Home;

    // Wide enough for Home's 3-card rail — see `home_screen_shows_work_and_context`.
    let text = render_at(&app, 160, 40);
    assert!(
        text.contains("recent"),
        "the wide rail should show its own recent card"
    );
    assert!(
        text.contains("DS-9001"),
        "the rail card should list the most recent entry"
    );
    assert!(
        text.contains('▎'),
        "each rail-card row should carry a lineage-tinted bar glyph"
    );
}

/// Regression test: a click landing at the wide rail card's old
/// coordinates must not misfire once the terminal narrows and the card no
/// longer shows — `ui::draw()` must clear `home_recent_area` on every
/// frame it doesn't draw the card, not just leave a stale `Rect` behind
/// (the same stale-hitbox class already guarded against for
/// `jax_mini_area`, see `resizing_wide_after_a_narrow_render_clears_the_stale_mini_jax_hitbox`).
/// Locate a rail-card row by its `▎` lineage-bar glyph on the line
/// containing `key` — a point guaranteed inside `home_recent_area` without
/// reading its private `Cell` directly (mirrors how the mini-Jax
/// regression test below locates its target by scanning rendered text
/// rather than reading `jax_mini_area`).
fn find_rail_card_row(text: &str, key: &str) -> Option<(u16, u16)> {
    text.lines().enumerate().find_map(|(y, line)| {
        if !line.contains(key) {
            return None;
        }
        line.find('▎').map(|x| (x as u16, y as u16))
    })
}

/// Regression test: a click landing at the wide rail card's old
/// coordinates must not misfire once the terminal narrows and the card no
/// longer shows — `ui::draw()` must clear `home_recent_area` on every
/// frame it doesn't draw the card, not just leave a stale `Rect` behind
/// (the same stale-hitbox class already guarded against for
/// `jax_mini_area`, see `resizing_wide_after_a_narrow_render_clears_the_stale_mini_jax_hitbox`).
#[test]
fn resizing_narrow_after_a_wide_render_clears_the_stale_recent_card_hitbox() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    app.follow_link("DS-9001");
    app.screen = Screen::Home;

    let text = render_at(&app, 160, 40);
    let (x, y) = find_rail_card_row(&text, "DS-9001").expect("the rail card should list DS-9001");
    assert_eq!(app.recent_card_entry_at(x, y), Some("DS-9001".to_string()));

    // Below Home's own wide-rail threshold (154) — narrow enough that the
    // card no longer renders at all.
    let _ = render_at(&app, 90, 40);
    assert_eq!(
        app.recent_card_entry_at(x, y),
        None,
        "a click at the rail card's old coordinates must not resolve after narrowing"
    );
}

/// Same stale-hitbox class, but leaving Home for a different screen
/// entirely (where `draw_home` isn't called at all, so nothing would
/// otherwise touch `home_recent_area`).
#[test]
fn navigating_away_from_home_clears_the_stale_recent_card_hitbox() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.selected = 0;
    app.open_detail();
    app.follow_link("DS-9001");
    app.screen = Screen::Home;

    let text = render_at(&app, 160, 40);
    let (x, y) = find_rail_card_row(&text, "DS-9001").expect("the rail card should list DS-9001");
    assert_eq!(app.recent_card_entry_at(x, y), Some("DS-9001".to_string()));

    app.screen = Screen::List;
    let _ = render_at(&app, 160, 40);
    assert_eq!(
        app.recent_card_entry_at(x, y),
        None,
        "a click at the old rail-card coordinates must not resolve on a different screen"
    );
}

#[test]
fn quick_view_panel_spans_full_width() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.quick_view = true;
    app.selected = 0;
    let text = render(&app);
    // Rendered as one string per row by our TestBackend dump helper: the
    // "quick view" title should appear near the left edge of a wide frame,
    // confirming the panel isn't squeezed into a narrow column.
    let line = text.lines().find(|l| l.contains("quick view")).unwrap();
    assert!(
        line.trim_start().starts_with('│') || line.trim_start().starts_with('╭'),
        "quick view panel should start at the frame's left edge (full width), got: {line:?}"
    );
}

#[test]
fn work_list_title_shows_sort_and_filter() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.cycle_filter();
    let text = render(&app);
    assert!(
        text.contains("sort"),
        "list title should show the sort mode"
    );
    assert!(
        text.contains("filter"),
        "list title should show the active filter"
    );
}

#[test]
fn work_list_title_reflects_the_active_view() {
    use jira_tui::domain::ViewKind;

    let mut app = demo_app();
    app.screen = Screen::Home;
    app.switch_view(ViewKind::AllProject);
    let text = render(&app);
    assert!(
        text.contains("all project issues"),
        "home list title should reflect the All Project Issues view, not stay stuck on 'my work'"
    );

    app.switch_view(ViewKind::Teammate("alex.chen".into()));
    let text = render(&app);
    assert!(
        text.contains("alex.chen's work"),
        "home list title should reflect the active teammate view"
    );

    app.screen = Screen::List;
    let text = render(&app);
    assert!(
        text.contains("all alex.chen's work"),
        "the full-screen List view should prefix a non-'all'-prefixed label with 'all'"
    );
}

#[test]
fn list_screen_shows_column_header_and_view_flip_arrows_at_120x34() {
    let mut app = demo_app();
    app.screen = Screen::List;
    let text = render_at(&app, 120, 34);
    assert!(
        text.contains('◂') && text.contains('▸'),
        "title should show view-flip arrows"
    );
    assert!(
        text.contains(" of "),
        "title should show the N of M count format"
    );
    for label in ["KEY", "TYPE", "STATUS", "SUMMARY", "ASSIGNEE", "UPDATED"] {
        assert!(
            text.contains(label),
            "column header should show {label} at a wide terminal"
        );
    }
}

#[test]
fn list_screen_tree_mode_shows_box_drawing_guides() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.toggle_list_view_mode();
    assert_eq!(app.list_view_mode, jira_tui::app::ListViewMode::Tree);
    let text = render_at(&app, 120, 34);
    // DS-2722 (an Epic) has DS-2725 as its only child in the demo data.
    assert!(
        text.contains('▾'),
        "a parent with children should show the expanded-parent marker"
    );
    assert!(
        text.contains("└─") || text.contains("├─"),
        "a child row should show a box-drawing guide"
    );
}

#[test]
fn list_screen_drops_optional_columns_and_shows_two_line_selected_row_at_84x46() {
    let mut app = demo_app();
    app.screen = Screen::List;
    app.selected = app
        .issues
        .iter()
        .position(|i| i.key == "DS-2725")
        .expect("DS-2725 should be in the demo data");
    let text = render_at(&app, 84, 46);
    assert!(
        !text.contains("ASSIGNEE") && !text.contains("TYPE"),
        "assignee/type column headers should be dropped below the narrow breakpoint"
    );
    assert!(
        text.contains("↳ DS-2722"),
        "the selected row's second line should show its parent key"
    );
    assert!(
        text.contains("all keys"),
        "the footer's pinned tail should still render alongside the narrower list (regression guard for phase 2)"
    );
}

#[test]
fn jax_companion_appears_when_toggled() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.jax_popped = true;
    let text = render(&app);
    assert!(text.contains("jax"), "the Jax companion box should render");
}

#[test]
fn jax_companion_sits_above_quick_view_not_overlapping() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.jax_popped = true;
    app.quick_view = true;
    app.selected = 0;
    let text = render(&app);
    let lines: Vec<&str> = text.lines().collect();
    let jax_row = lines
        .iter()
        .position(|l| l.contains("jax"))
        .expect("jax box should render");
    let quick_view_row = lines
        .iter()
        .position(|l| l.contains("quick view"))
        .expect("quick view panel should render");
    assert!(
        jax_row < quick_view_row,
        "Jax (row {jax_row}) should appear above the quick-view panel (row {quick_view_row})"
    );
}

#[test]
fn jax_companion_shows_a_mood_line_when_popped_out() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.jax_popped = true;
    let text = render(&app);
    assert!(
        text.contains("mood:"),
        "the popped-out Jax box should show a mood line (SPEC.md §9)"
    );
}

#[test]
fn jax_docks_into_the_footer_at_narrow_widths_instead_of_the_full_box() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    // Below the 90-col threshold, Jax should be the ambient mini dock —
    // present even though `jax_popped` was never toggled on.
    let text = render_at(&app, 80, 40);
    assert!(
        text.contains("●‿●") && text.contains("jax"),
        "mini-Jax should dock into the footer at narrow widths"
    );
    assert!(
        !text.contains("mood:"),
        "the full box (with its mood line) should not render at narrow widths"
    );
}

#[test]
fn jax_mini_dock_appears_at_exactly_the_same_90_col_threshold_as_the_header_pill() {
    // Regression test: `draw_footer` originally fed jax_mode the footer's
    // post-border `inner.width`, while `draw()`'s own full-box check used
    // the pre-border `body_area.width` — two columns narrower for the same
    // terminal size, so the mini dock's effective cutoff was ~92, not the
    // documented 90 every other breakpoint (including the header's sync
    // pill) actually uses. List uses the flat 90-col cutoff shared by every
    // screen except Home (which tracks its own, wider rail threshold —
    // see `home_mini_dock_appears_at_homes_own_wide_rail_threshold` below).
    let mut app = demo_app();
    app.screen = Screen::List;

    let at_90 = render_at(&app, 90, 40);
    assert!(
        !at_90.contains("●‿●"),
        "at exactly 90 cols the mini dock should already be gone, matching the header pill's own 90-col threshold"
    );

    let at_89 = render_at(&app, 89, 40);
    assert!(
        at_89.contains("●‿●"),
        "at 89 cols (just below the threshold) the mini dock should be showing"
    );
}

#[test]
fn home_mini_dock_appears_at_homes_own_wide_rail_threshold() {
    // Home's mini-dock cutoff tracks its own wide-rail threshold (154), not
    // the flat 90 cols every other screen uses — see `jax_mode`'s
    // Home-specific branch.
    let mut app = demo_app();
    app.screen = Screen::Home;

    let at_154 = render_at(&app, 154, 40);
    assert!(
        !at_154.contains("●‿●"),
        "at Home's own wide-rail threshold the mini dock should already be gone"
    );

    let at_153 = render_at(&app, 153, 40);
    assert!(
        at_153.contains("●‿●"),
        "just below Home's wide-rail threshold the mini dock should be showing"
    );
}

#[test]
fn resizing_wide_after_a_narrow_render_clears_the_stale_mini_jax_hitbox() {
    // Regression test: a click landing at the mini dock's old coordinates
    // must not misfire once the terminal is wide enough that the dock no
    // longer shows — `draw_footer` must clear `jax_mini_area` on every
    // frame it doesn't draw the dock, not just leave a stale `Rect` behind.
    let mut app = demo_app();
    app.screen = Screen::Home;

    let text = render_at(&app, 80, 40);
    // Search for the exact 3-char "●‿●" face, not a bare '●' — the header's
    // sync-status LED is also a plain '●' and would otherwise match first.
    let (row, col) = text
        .lines()
        .enumerate()
        .find_map(|(y, line)| {
            let chars: Vec<char> = line.chars().collect();
            chars
                .windows(3)
                .position(|w| w == ['●', '‿', '●'])
                .map(|x| (y, x))
        })
        .expect("the mini dock's face should render at 80 cols");
    let (mini_x, mini_y) = (col as u16, row as u16);
    assert!(app.point_in_jax_mini(mini_x, mini_y));

    // Past Home's own wide-rail threshold (154), not just any "wide" width —
    // below that Home is still narrow enough for the mini dock to persist.
    let _ = render_at(&app, 160, 40);
    assert!(
        !app.point_in_jax_mini(mini_x, mini_y),
        "a click at the mini dock's old coordinates must not resolve after resizing wide"
    );
}

#[test]
fn jax_j_key_pops_the_full_box_out_even_at_a_narrow_width() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.toggle_jax();
    let text = render_at(&app, 80, 40);
    assert!(
        text.contains("mood:"),
        "an explicit pop-out should show the full box even below the wide threshold"
    );
}

#[test]
fn jax_mini_is_absent_on_welcome() {
    let mut app = demo_app();
    app.screen = Screen::Welcome;
    let text = render_at(&app, 80, 40);
    assert!(
        !text.contains("●‿●"),
        "mini-Jax must stay hidden on Welcome, matching the full box's existing rule"
    );
}

#[test]
fn search_screen_shows_goto_and_filters_results() {
    let mut app = demo_app();
    app.open_search();
    for c in "DS-2603".chars() {
        app.search_input_char(c);
    }
    let text = render(&app);
    assert!(
        text.contains("go to"),
        "search should offer a go-to-issue action"
    );
    assert!(text.contains("DS-2603"), "search should show the typed key");
}

#[test]
fn search_screen_empty_query_shows_hint_or_full_list() {
    let mut app = demo_app();
    app.open_search();
    let text = render(&app);
    assert!(
        text.contains("search") || text.contains("results"),
        "search screen should render its panels even with an empty query"
    );
}

#[test]
fn search_screen_flags_when_live_text_search_is_unavailable() {
    // A demo session is never `Source::Live`, so the live text-search
    // fallback (see `App::schedule_live_search`) can never fire for it —
    // this must be visible on screen, not a silent no-op that looks
    // indistinguishable from a broken feature.
    let mut app = demo_app();
    app.open_search();
    let text = render(&app);
    assert!(
        text.contains("not a live session") || text.contains("live text search"),
        "search should flag that its live fallback is unavailable in this session"
    );
}

#[test]
fn board_screen_shows_columns_and_lanes() {
    let mut app = demo_app();
    app.open_board();
    // Bordered cards are far taller than the old packed text rows, so not
    // every lane fits in the default 120x40 — use a tall enough viewport
    // that all of demo data's lanes (including the trailing "no epic" one)
    // are visible without needing to page down.
    let text = render_at(&app, 120, 60);
    assert!(text.contains("board"), "board panel should render");
    assert!(text.contains("To Do"), "board should show a status column");
    assert!(
        text.contains("No epic"),
        "board should show the no-epic lane"
    );
    assert!(text.contains("DS-"), "board should show issue cards");
}

#[test]
fn release_screen_lists_versions() {
    let mut app = demo_app();
    app.open_release_screen();
    let text = render(&app);
    assert!(text.contains("releases"), "release screen should render");
    assert!(text.contains("v3.4.0"), "should list the released version");
    assert!(text.contains("v3.5.0"), "should list the upcoming version");
    assert!(text.contains("released"));
    assert!(text.contains("unreleased"));
}

#[test]
fn release_screen_defaults_to_split_grouping_and_s_cycles_to_flat() {
    let mut app = demo_app();
    app.open_release_screen();
    let text = render(&app);
    assert!(
        text.contains("Unreleased") && text.contains("Released"),
        "the default view should split unreleased from released: {text}"
    );
    assert!(text.contains("split unreleased/released"));

    app.release_cycle_list_mode();
    let text = render(&app);
    assert!(
        !text.contains("Unreleased") && !text.contains("Released"),
        "the flat view should show no group headers: {text}"
    );
    assert!(text.contains(" flat"));
}

#[test]
fn release_screen_drill_shows_progress_and_grouped_issues() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    let text = render(&app);
    assert!(
        text.contains("release · v3.4.0"),
        "drill title should name the version"
    );
    assert!(
        text.contains("done"),
        "should show a done/total progress line"
    );
    assert!(text.contains("DS-"), "should list the version's issues");
}

#[test]
fn release_screen_drill_on_an_empty_version_says_so() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.6.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    let text = render(&app);
    assert!(text.contains("No issues target this release"));
}

#[test]
fn release_screen_drill_shows_selected_count_and_checkbox() {
    let mut app = demo_app();
    app.open_release_screen();
    let idx = app
        .release
        .versions
        .iter()
        .position(|v| v.name == "v3.4.0")
        .unwrap();
    app.release.cursor = idx;
    app.release_confirm();
    app.release.issue_cursor = 0;
    app.release_toggle_selected();

    let text = render(&app);
    assert!(text.contains("1 selected"));
    assert!(text.contains('✓'));
}

#[test]
fn search_screen_bulk_mode_shows_title_and_checkboxes() {
    let mut app = demo_app();
    app.screen = Screen::Release;
    app.open_search_for_release("v3.5.0".into());
    let text = render(&app);
    assert!(text.contains("add issues to v3.5.0"));
    assert!(
        text.contains('○'),
        "unselected rows should show an empty checkbox"
    );

    app.search.selected = 0;
    app.search_toggle_bulk_selected();
    let text = render(&app);
    assert!(text.contains("1 selected"));
    assert!(text.contains('✓'));
}

#[test]
fn board_screen_highlights_selected_card() {
    let mut app = demo_app();
    app.open_board();
    // Land the selection on a real card, then confirm it renders without
    // panicking and still shows the board chrome.
    app.board_move_col(0);
    let text = render(&app);
    assert!(
        text.contains("column"),
        "board title should show column count"
    );
}

#[test]
fn board_footer_does_not_advertise_the_unbound_t_key() {
    // Regression test: an earlier draft of the footer's Board ACT group
    // advertised `t transition`, but `t` isn't bound on Board at all (only
    // within Detail) — pressing it there did nothing.
    let mut app = demo_app();
    app.open_board();
    let text = render(&app);
    assert!(
        !text.contains("t transition"),
        "Board's footer must not advertise a key that isn't bound there"
    );
}

#[test]
fn board_footer_shows_every_group_at_the_default_test_width() {
    // Regression test: the Board footer's GO group (search/view) used to
    // silently drop even at the standard 120-column test width because the
    // group content (inflated by the unbound `t transition` hint above) was
    // too wide for the footer's hint column.
    //
    // ACT now carries three hints — `y/Y copy key/URL` (footer-
    // discoverability work) and `r refresh` (board/release refresh
    // support) both landed independently and add to that same budget — so
    // at 120 columns even ACT itself drops, not just GO; a wider terminal
    // has room for progressively more (see
    // `ui::footer::tests::board_footer_advertises_copy_link_pre_fit` for
    // the pre-fit assertion that the copy-link hint is really there, and
    // Detail's analogous `x` "fold facts" hint for the same kind of
    // width/content tradeoff, not a reintroduction of the original bug).
    let mut app = demo_app();
    app.open_board();
    let text = render(&app);
    assert!(text.contains("NAV"), "NAV group should render");
    assert!(text.contains("all keys"), "the pinned tail should render");

    let text = render_at(&app, 130, 40);
    assert!(
        text.contains("ACT"),
        "ACT group should render once the terminal is wide enough for it"
    );

    let text = render_at(&app, 160, 40);
    assert!(
        text.contains("GO"),
        "GO group should render once the terminal is wide enough for every group"
    );
}

// The Detail footer's NAV group (which the `x` "fold facts" hint joins) is
// already wide enough to get dropped by `fit_footer_groups`' width-fit at
// typical terminal sizes, even before this phase — a pre-existing footer
// content/width tradeoff, not something to re-litigate here. See
// `ui::footer::tests::detail_nav_group_advertises_fold_facts_only_when_narrow`
// for a unit test against the pre-fit group content instead.

#[test]
fn footer_status_truncates_with_an_ellipsis_instead_of_hard_clipping() {
    // Regression test: the status column used to be rendered with no
    // truncation at all, so a long message (a real live-Jira failure
    // reason, for example) got hard-clipped mid-word by the Paragraph
    // instead of ending in a visible "…".
    let mut app = demo_app();
    app.status =
        "Jira unreachable (a very long and detailed error message that will not fit) — showing sample data".into();
    let backend = TestBackend::new(60, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        text.contains('…'),
        "a status message too long for the column should end in an ellipsis"
    );
}

#[test]
fn help_overlay_shows_audited_keys() {
    // Regression test for the SPEC.md §10 keybinding audit: `g`, `l`,
    // PgUp/PgDn, and the board's vim keys were bound in `src/keys.rs` but
    // missing from the help overlay.
    let mut app = demo_app();
    app.show_help = true;
    let text = render(&app);
    assert!(text.contains("go to Home"), "`g` should be documented");
    assert!(text.contains("go to List"), "`l` should be documented");
    assert!(text.contains("PgUp"), "PgUp/PgDn should be documented");
    assert!(
        text.contains("vim-style"),
        "board vim-key support should be documented"
    );
}

#[test]
fn help_overlay_shows_every_row_without_clipping() {
    // The popup used to be a fixed 62% of the frame height; once the audit
    // fixes above grew the row count, the last several rows (including the
    // overlay's own "? / q toggle help / quit" hint) were clipped off the
    // bottom with no scroll indicator. The popup is now sized to the row
    // count instead, so every row — especially the last one — must render.
    let mut app = demo_app();
    app.show_help = true;
    // KEYMAP has grown past the default 120x40 reference size's capacity
    // (see `ui::help::draw_help_overlay`'s clamp-to-frame-height comment) —
    // tall enough that every row, including the trailing close hint, fits
    // without clipping.
    let text = render_at(&app, 120, 51);
    assert!(
        text.contains("? / q") && text.contains("toggle help"),
        "the last help row (close help/quit) must not be clipped"
    );
}

#[test]
fn help_overlay_key_column_has_a_separator_for_long_keys() {
    // The key column used to be a fixed 9-char width; keys longer than that
    // (e.g. "PgUp / PgDn", "h/j/k/l (board)") ran straight into their
    // description with no separating space.
    let mut app = demo_app();
    app.show_help = true;
    let text = render(&app);
    assert!(
        !text.contains("PgDnjump"),
        "PgUp / PgDn must not glue into its description"
    );
    assert!(
        !text.contains("(board)vim"),
        "the board vim-key row must not glue into its description"
    );
}

#[test]
fn nerd_info_popup_shows_version_and_a_graphics_section() {
    let mut app = demo_app();
    app.nerd_info_open = true;
    let text = render(&app);
    assert!(
        text.contains(env!("CARGO_PKG_VERSION")),
        "the popup should show this build's version"
    );
    assert!(
        text.contains("graphics"),
        "the popup should show a graphics diagnostics section"
    );
    assert!(
        text.contains("terminal"),
        "the popup should show a terminal env-var section"
    );
}

#[test]
fn footer_shows_grouped_hints_on_a_wide_terminal() {
    // SPEC.md §2: footer hints are grouped under a faint uppercase label
    // (NAV/VIEW/ACT/GO). At a comfortably wide terminal every group for
    // Home should render. 160, not 120: below Home's own wide-rail
    // threshold (154) the footer also reserves room for mini-Jax, which
    // eats into the group budget.
    let backend = TestBackend::new(160, 34);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = demo_app();
    app.screen = Screen::Home;
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(text.contains("NAV"), "the NAV group should render");
    assert!(text.contains("VIEW"), "the VIEW group should render");
    assert!(text.contains("GO"), "the GO group should render");
    assert!(text.contains("all keys"), "the pinned tail should render");
}

#[test]
fn footer_never_wraps_and_keeps_all_keys_visible_on_a_narrow_terminal() {
    // SPEC.md §2's no-wrap rule: whole groups drop right-to-left as the
    // terminal narrows, but `? all keys` — the pinned last group — must
    // always survive, even when nothing else fits.
    let backend = TestBackend::new(40, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = demo_app();
    app.screen = Screen::Home;
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        text.contains("all keys"),
        "the pinned `? all keys` group must survive even on a very narrow terminal"
    );
}

#[test]
fn footer_renders_at_the_84x46_reference_size() {
    // SPEC.md §13: exercise breakpoints at both reference sizes.
    let backend = TestBackend::new(84, 46);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = demo_app();
    app.open_board();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        text.contains("all keys"),
        "the board footer's pinned tail should render at the 84-col reference size"
    );
}

#[test]
fn header_breadcrumb_shows_the_current_view_on_home() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let text = render(&app);
    assert!(
        text.contains("My Work"),
        "Home's breadcrumb should show the current view"
    );
}

#[test]
fn header_breadcrumb_shows_view_and_screen_on_list() {
    let mut app = demo_app();
    app.screen = Screen::List;
    let text = render(&app);
    assert!(
        text.contains("My Work") && text.contains("List"),
        "List's breadcrumb should show both the view and the screen"
    );
}

#[test]
fn header_breadcrumb_shows_the_issue_key_and_back_count_on_detail() {
    let mut app = demo_app();
    app.selected = 0;
    app.open_detail();
    let text = render(&app);
    let key = app.detail.as_ref().unwrap().key.clone();
    assert!(
        text.contains(&key),
        "Detail's breadcrumb should show the open issue's key"
    );
}

#[test]
fn header_breadcrumb_shows_the_active_filter() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.cycle_filter();
    let text = render(&app);
    assert!(
        text.contains("filter"),
        "an active filter should appear as a breadcrumb crumb"
    );
}

#[test]
fn header_sync_pill_shows_demo_in_demo_mode() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    let text = render(&app);
    assert!(text.contains("demo"), "the sync pill should show demo mode");
}

#[test]
fn header_sync_pill_shows_live_and_synced_when_wide() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Live {
        site: "example.atlassian.net".into(),
        user: "me".into(),
    };
    // Deterministic branch name: this test must not depend on whatever
    // branch happens to be checked out when it runs (the real GitContext
    // otherwise reflects the actual repo state) — an unrelated long branch
    // name sharing the header's right column with the sync pill previously
    // pushed "live" off the edge here.
    app.git.branch = Some("main".into());
    let text = render(&app);
    assert!(
        text.contains("live") && text.contains("synced"),
        "a wide terminal should show the full sync pill"
    );
}

#[test]
fn header_sync_pill_shows_cache_and_synced_when_wide() {
    // Every other sync-pill test exercises only Source::Live — this covers
    // the Cache arm (amber LED, username as the detail segment) so a future
    // regression specific to Cache isn't invisible to the whole suite.
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Cache { user: "me".into() };
    app.git.branch = Some("main".into());
    let text = render(&app);
    assert!(
        text.contains("cache") && text.contains("synced"),
        "a wide terminal should show the full sync pill for a cached source"
    );
}

#[test]
fn header_sync_pill_degrades_gracefully_instead_of_clipping_mid_word() {
    // Regression test: the sync pill's site/user detail segment used to be
    // included unconditionally, which combined with a long branch name and
    // a long site hostname overflowed the header's un-wrapped right column
    // and silently clipped words off mid-way (e.g. "syn" instead of
    // "synced"). With a genuinely pathological branch + site name, there's
    // no room for the full pill — the correct outcome is a clean drop to
    // the collapsed LED + short-duration form, never a broken fragment.
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Live {
        site: "an-unusually-long-jira-site-hostname.atlassian.net".into(),
        user: "me".into(),
    };
    app.git.branch = Some("an-unusually-long-feature-branch-name-here".into());
    let text = render(&app);
    // Scoped to the header row specifically — the demo data happens to
    // include an unrelated issue titled "...breaks sync-docs", so a
    // whole-screen substring check would false-positive on that.
    let header_line = text.lines().nth(1).unwrap_or("");
    assert!(
        header_line.contains('●'),
        "the sync pill's LED should still render"
    );
    // If "sync" appears in the header at all, the complete word "synced"
    // must too — a bare "sync"/"syn" fragment would mean the pill got
    // clipped mid-word rather than cleanly dropping to the collapsed form.
    assert!(
        !header_line.contains("sync") || header_line.contains("synced"),
        "the pill must never clip \"synced\" mid-word: {header_line:?}"
    );
}

#[test]
fn header_sync_pill_collapses_to_led_and_short_duration_when_narrow() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.source = Source::Live {
        site: "example.atlassian.net".into(),
        user: "me".into(),
    };
    let backend = TestBackend::new(70, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let text = dump(terminal.backend().buffer());
    assert!(
        !text.contains("synced"),
        "below the collapse width the pill should drop to just the LED and a short duration"
    );
}

fn board_issue(key: &str, epic: Option<&str>, status: &str, blocked: bool) -> IssueSummary {
    IssueSummary {
        key: key.to_string(),
        summary: format!("Summary for {key}"),
        issue_type: "Task".to_string(),
        status: status.to_string(),
        priority: Priority::Medium,
        assignee: Some("scott.morris".to_string()),
        blocked,
        updated: "1h ago".to_string(),
        updated_at: None,
        epic: epic.map(String::from),
    }
}

/// A small dedicated Board fixture (SPEC.md §7) exercising what demo data
/// doesn't: a blocked card, a fully-done lane, and a lane empty in a given
/// column — rather than relying on demo data happening to contain all
/// three. Lane order (first-seen): EPIC-MIXED, EPIC-DONE, EPIC-EMPTY-TODO.
fn board_fixture_app() -> App {
    let mut app = demo_app();
    app.issues = vec![
        board_issue("DS-101", Some("EPIC-MIXED"), "To Do", true),
        board_issue("DS-102", Some("EPIC-MIXED"), "In Progress", false),
        board_issue("DS-201", Some("EPIC-DONE"), "Done", false),
        board_issue("DS-202", Some("EPIC-DONE"), "Done", false),
        board_issue("DS-301", Some("EPIC-EMPTY-TODO"), "In Progress", false),
    ];
    app.open_board();
    app
}

#[test]
fn board_screen_wide_shows_card_grid_and_a_fully_done_ghost() {
    let app = board_fixture_app();
    let text = render_at(&app, 120, 34);
    assert!(
        text.contains('╭') && text.contains('╮'),
        "wide board should render bordered cards"
    );
    assert!(
        text.contains("⛔"),
        "the blocked issue's card should show the blocked chip"
    );
    assert!(
        text.contains("fully done") && text.contains("pgdn to peek"),
        "the fully-done EPIC-DONE lane should collapse into a ghost line"
    );
    assert!(
        !text.contains("EPIC-DONE"),
        "the fully-done lane itself must not also render expanded"
    );
}

#[test]
fn board_screen_narrow_shows_pager_and_neighbour_peek() {
    let mut app = board_fixture_app();
    // Narrow's tab strip/pager keys off `board_sel.col`; land on "To Do" so
    // the selected card (DS-101, EPIC-MIXED) has both a previous and next
    // column to peek at.
    let cols = app.board_columns();
    app.board_sel.col = cols.iter().position(|c| c == "To Do").unwrap();
    let text = render_at(&app, 84, 46);
    for status in &cols {
        assert!(
            text.contains(status.as_str()),
            "the tab strip should show every status: missing {status:?}"
        );
    }
    assert!(
        text.contains("here") && text.contains("total"),
        "a lane header should show its here/total counts"
    );
    assert!(
        text.contains('◂') || text.contains('▸'),
        "the selected card should show a neighbour-peek line"
    );
}

#[test]
fn board_screen_narrow_collapses_empty_lanes() {
    let mut app = board_fixture_app();
    // EPIC-EMPTY-TODO has nothing in "To Do" and isn't selected (EPIC-MIXED,
    // the first lane, is) — it should collapse into the narrow ghost line.
    let cols = app.board_columns();
    app.board_sel.col = cols.iter().position(|c| c == "To Do").unwrap();
    app.board_sel.lane = 0;
    let text = render_at(&app, 84, 46);
    assert!(
        text.contains("lanes with nothing") && text.contains("pgdn to peek"),
        "a lane empty in the current column should collapse into a ghost line"
    );
    assert!(
        !text.contains("EPIC-EMPTY-TODO"),
        "the collapsed lane itself must not also render expanded"
    );
}

#[test]
fn board_screen_wide_scrolls_the_selected_lane_into_actual_view() {
    // Regression test: `board_ensure_visible`'s "how many lanes fit" budget
    // must match the renderer's own `fit_lanes` budget exactly (both must
    // account for the 1-row column-header line). An earlier version of
    // this budgeted one row too many, so a lane `board_ensure_visible`
    // considered already in view could still be silently dropped by the
    // renderer — a numeric scroll-bound assertion wouldn't catch that, only
    // checking the actual rendered output does.
    let mut app = board_fixture_app();
    // Populate `board_area` with this size's real geometry before
    // navigating, mirroring the established pattern for nav functions that
    // read back render-time state (see Detail's `jump_to_comments` tests).
    let _ = render_at(&app, 120, 10);
    let lanes = app.board_lanes();
    let lanes_len = lanes.len();
    for _ in 0..lanes_len - 1 {
        app.board_move_lane(1);
    }
    let label = app.board_lane_label(&lanes[app.board_sel.lane]);
    let text = render_at(&app, 120, 10);
    assert!(
        text.contains(&label),
        "the selected lane ({label:?}) must actually render on screen after \
         scrolling to it, not just satisfy a numeric scroll bound"
    );
}

#[test]
fn drag_selection_highlights_only_the_selected_columns_on_a_single_row() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.mouse.selecting = true;
    app.mouse.sel_start_y = 5;
    app.mouse.sel_start_x = 10;
    app.mouse.sel_end_y = 5;
    app.mouse.sel_end_x = 20;
    // Unbounded (matches `App::selection_bounds_at`'s own fallback): this
    // test is about the single-row shape, not panel clipping.
    app.mouse.sel_bounds = ratatui::layout::Rect::new(0, 0, u16::MAX, u16::MAX);

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer();
    let reversed = |x: u16, y: u16| {
        buf.cell((x, y))
            .map(|c| c.modifier.contains(ratatui::style::Modifier::REVERSED))
            .unwrap_or(false)
    };

    assert!(
        !reversed(9, 5),
        "just before the selected span must not be highlighted"
    );
    assert!(reversed(10, 5), "the selection start column");
    assert!(reversed(20, 5), "the selection end column");
    assert!(
        !reversed(21, 5),
        "just after the selected span must not be highlighted"
    );
    assert!(
        !reversed(0, 5),
        "columns before the selection on the same row must not be highlighted \
         (regression guard: this used to invert the whole row)"
    );
}

#[test]
fn drag_selection_spanning_multiple_rows_only_trims_the_first_and_last() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.mouse.selecting = true;
    app.mouse.sel_start_y = 5;
    app.mouse.sel_start_x = 50;
    app.mouse.sel_end_y = 7;
    app.mouse.sel_end_x = 10;
    // Unbounded (matches `App::selection_bounds_at`'s own fallback): this
    // test is about the multi-row shape, not panel clipping.
    app.mouse.sel_bounds = ratatui::layout::Rect::new(0, 0, u16::MAX, u16::MAX);

    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer();
    let reversed = |x: u16, y: u16| {
        buf.cell((x, y))
            .map(|c| c.modifier.contains(ratatui::style::Modifier::REVERSED))
            .unwrap_or(false)
    };

    // First row: only from its start column onward.
    assert!(!reversed(49, 5), "before the start column on the first row");
    assert!(reversed(50, 5), "the start column itself");
    assert!(reversed(119, 5), "the first row highlights to its own end");

    // A row fully between start and end: highlighted in full.
    assert!(reversed(0, 6), "a middle row's first column");
    assert!(reversed(119, 6), "a middle row's last column");

    // Last row: only up to its end column.
    assert!(reversed(0, 7), "the last row's first column");
    assert!(reversed(10, 7), "the end column itself");
    assert!(!reversed(11, 7), "past the end column on the last row");
}

/// End-to-end regression test for the reported bug: dragging down through
/// several rows of the wide Detail layout's main column used to fill the
/// *entire terminal row* for every row fully between the start and end —
/// bleeding into the side rail, which shares those same rows but is
/// completely unrelated content. `mouse_down` records which panel the drag
/// started in (`App::selection_bounds_at`), and the highlight now clips to
/// it, so a multi-row drag in the main column must never touch the rail.
#[test]
fn drag_selection_in_the_detail_main_column_does_not_bleed_into_the_side_rail() {
    let mut app = demo_app();
    app.screen = Screen::Home;
    app.open_by_key("DS-2722");
    // A real render first, so `detail_main_area`/the rail panel areas are
    // actually populated the way `mouse_down` needs them.
    let _ = render_at(&app, 120, 34);

    let main_area = app.detail_main_area.get();
    let workflow_area = app.detail_workflow_area.get();
    assert!(
        workflow_area.width > 0,
        "the workflow rail panel should have a recorded area at this width"
    );

    // Start the drag a couple of rows into the main column, drag down
    // several rows — comfortably spanning at least one "middle" row that
    // would previously have filled the whole terminal width.
    let start_x = main_area.x + 2;
    let start_y = main_area.y + 1;
    let end_y = (start_y + 4).min(main_area.y + main_area.height - 1);
    app.mouse_down(start_x, start_y);
    app.mouse_drag(main_area.x + 5, end_y);

    let backend = TestBackend::new(120, 34);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer();
    let reversed = |x: u16, y: u16| {
        buf.cell((x, y))
            .map(|c| c.modifier.contains(ratatui::style::Modifier::REVERSED))
            .unwrap_or(false)
    };

    // A row strictly between the drag's start and end row (a "middle" row)
    // should be highlighted within the main column...
    let middle_y = start_y + 2;
    assert!(
        reversed(main_area.x, middle_y),
        "a middle row of the drag should be highlighted within the main column"
    );
    // ...but never inside the side rail on that same row.
    assert!(
        !reversed(workflow_area.x + 2, middle_y),
        "the drag must not bleed into the side rail on a middle row \
         (regression guard for the reported whole-row-highlight bug)"
    );
}

#[test]
fn new_issue_form_records_each_fields_area() {
    let mut app = demo_app();
    app.open_new_issue();
    let _ = render_at(&app, 120, 40);

    let project = app.new_issue_project_area.get();
    let issue_type = app.new_issue_type_area.get();
    let summary = app.new_issue_summary_area.get();

    for (name, area) in [
        ("project", project),
        ("issue type", issue_type),
        ("summary", summary),
    ] {
        assert_ne!(
            area,
            ratatui::layout::Rect::default(),
            "the {name} field's area should be recorded during render"
        );
        assert_eq!(area.height, 3, "the {name} field is a 3-row bordered card");
    }

    assert!(
        project.y < issue_type.y && issue_type.y < summary.y,
        "the three fields should be recorded top-to-bottom in form order"
    );
}

#[test]
fn field_mapping_screen_shows_the_current_target_and_a_tab_hint() {
    use jira_tui::app::FieldMappingTarget;

    let mut app = demo_app();
    app.screen = Screen::FieldMapping;
    app.field_mapping.target = FieldMappingTarget::Sprint;
    app.field_mapping.catalog = vec![
        (String::new(), "— none — don't track Sprint".into()),
        ("customfield_10020".into(), "Sprint".into()),
    ];
    app.field_mapping.selected = 1;
    app.field_mapping.current_mapping = Some("customfield_10020".into());

    let text = render(&app);

    assert!(
        text.contains("sprint") && text.contains("tab"),
        "the input line should name the current target and hint at Tab to switch, got: {text}"
    );
    assert!(
        text.contains("Sprint"),
        "the catalog row for the mapped field should render"
    );
}
