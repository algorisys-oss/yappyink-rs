//! The overlay's own chrome, shared by every backend.
//!
//! The frame and mode badge, the toolbar, the colour swatches, tooltips, the
//! selection handles and the text caret. Everything the user sees that is *not*
//! their annotations.
//!
//! # Why this is a crate
//!
//! `architecture.md` proposed an `ink-ui` boundary and T013 deferred it, on the
//! grounds that one backend does not justify a crate. A second backend does.
//! The alternative was copying five hundred lines of painting into the Windows
//! adapter, which guarantees the two drift apart and turns one product into two
//! that merely resemble each other.
//!
//! Two things follow from living here rather than in an adapter, and both are
//! improvements:
//!
//! - **It is testable.** Inside the Wayland adapter none of this could be
//!   reached without a compositor. Here a `Canvas` is a slice of bytes, so the
//!   chrome can be painted and inspected in an ordinary test. Six "correct in
//!   the model, absent on screen" bugs came out of that layer being untestable
//!   (`docs/learning.md` §1).
//! - **It cannot reach for the platform.** This crate depends on `ink-core`,
//!   `ink-app` and `ink-render` and nothing else, so a window handle or a
//!   protocol type cannot creep in.
//!
//! # What belongs here
//!
//! Chrome is painted after the document and is never stored in it. An ink-only
//! export must exclude all of it (FR-024). If something ought to survive a
//! save, it is not chrome and does not belong in this crate.

use ink_app::{Button, Controller, Icon, Mode, Tool, Toolbar};
use ink_core::{Document, LogicalPoint, LogicalRect, Object, Opacity, Rgb, Style};
use ink_render::{Canvas, Painter, Scale};

/// Draws the frame and mode badge.
///
/// Not decoration for its own sake. The surface is transparent and
/// undecorated, so without an outline the user cannot tell where it is, which
/// makes it impossible to aim at. The badge answers "which mode am I in?"
/// without looking away at a terminal.
///
/// This is chrome, not document content: it is painted after the document,
/// never stored in it, and an ink-only export must exclude it (FR-024). A real
/// toolbar is T013.
pub fn paint_chrome(canvas: &mut Canvas, mode: Mode, style: Style, tool: Tool) {
    // Premultiplied, memory order B, G, R, A.
    let frame = match mode {
        // Cyan: this surface is taking your pointer.
        Mode::Draw => [0x30, 0x30, 0x00, 0x30],
        // Amber: your pointer belongs to whatever is underneath.
        Mode::PassThrough => [0x00, 0x20, 0x30, 0x30],
        // Parked is nothing but toolbar, so a frame around the whole surface
        // would just be a second border around it.
        Mode::Parked => return,
        // Unreachable while a frame is being painted: Hidden has no surface.
        Mode::Hidden => return,
    };

    let (width, height) = (i64::from(canvas.width()), i64::from(canvas.height()));
    let thickness = 2;
    canvas.fill_rect(0, 0, width, thickness, frame);
    canvas.fill_rect(0, height - thickness, width, thickness, frame);
    canvas.fill_rect(0, 0, thickness, height, frame);
    canvas.fill_rect(width - thickness, 0, thickness, height, frame);

    // The mode swatch.
    let mode_colour = match mode {
        Mode::Draw => [0xC0, 0xC0, 0x00, 0xC0],
        Mode::PassThrough => [0x00, 0x80, 0xC0, 0xC0],
        Mode::Parked | Mode::Hidden => return,
    };
    canvas.fill_rect(10, 10, 18, 18, mode_colour);

    // The tool swatch: the colour and width about to be drawn, so the user can
    // see the setting rather than remember it. A real toolbar is T013.
    let alpha = style.opacity.get();
    let channel = |value: u8| (f64::from(value) * alpha).round().clamp(0.0, 255.0) as u8;
    let ink = [
        channel(style.color.b),
        channel(style.color.g),
        channel(style.color.r),
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ];
    let bar = style.width.get().round().max(1.0) as i64;
    canvas.fill_rect(34, 10 + (18 - bar).max(0) / 2, 40, bar.min(18), ink);

    // A pip count marks which tool is selected, so the tools are not told
    // apart by colour alone. A toolbar showing them properly is T013.
    let pips = match tool {
        Tool::Pen => 0,
        Tool::Highlighter => 1,
        Tool::Line => 2,
        Tool::Arrow => 3,
        Tool::Rectangle => 4,
        Tool::Ellipse => 5,
        Tool::Eraser => 6,
        Tool::Select => 7,
        Tool::Text => 8,
    };
    for pip in 0..pips {
        canvas.fill_rect(80 + i64::from(pip) * 12, 14, 8, 8, ink);
    }
}

/// How much of its opacity an object keeps while it is being dragged away
/// from.
///
/// Faint enough to read as "was here" rather than as a second object, and not
/// so faint it disappears, which would lose the reference point the user is
/// dragging relative to.
const DRAG_ORIGIN_OPACITY: f64 = 0.25;

/// The same object, faded, for showing where a dragged selection came from.
///
/// Returns `None` only if the faded opacity is somehow out of range, which the
/// constant rules out; the object is then simply not drawn rather than drawn
/// at full strength and confusing the preview.
pub fn faded(object: &Object) -> Option<Object> {
    let style = object.style();
    let opacity = Opacity::new(style.opacity.get() * DRAG_ORIGIN_OPACITY)?;
    Some(Object::new(
        object.id(),
        object.output().clone(),
        Style::new(style.color, style.width, opacity),
        object.shape().clone(),
    ))
}

/// Draws the selection: a dashed rectangle, corner handles, and a live preview
/// of whatever drag is in flight.
///
/// The preview is drawn from the real objects transformed on the fly, so what
/// the user sees during a drag is what the edit will produce. Nothing here
/// touches the document.
pub fn paint_selection(
    canvas: &mut Canvas,
    controller: &Controller,
    document: &Document,
    painter: &mut Painter,
    scale: Scale,
) {
    if controller.selection().is_empty() || !controller.toolbar_visible() {
        return;
    }
    let marker = [0xFF, 0xC0, 0x40, 0xE0];

    // The moved or scaled objects, drawn where they are going.
    if let Some(drag) = controller.selection_drag() {
        for object in document
            .objects()
            .filter(|o| controller.selection().contains(&o.id()))
        {
            let moved = match drag {
                ink_app::SelectionDrag::Move { dx, dy } => object.shape().translated(dx, dy),
                ink_app::SelectionDrag::Scale { anchor, sx, sy } => {
                    object.shape().scaled(anchor, sx, sy)
                }
            };
            if let Some(shape) = moved {
                painter.paint_object(&object.with_shape(shape), canvas, scale);
            }
        }
    }

    let Some(bounds) = controller.selection_bounds() else {
        return;
    };
    let to_px = |value: f64| value * scale.get();
    let (x0, y0) = (to_px(bounds.min.x), to_px(bounds.min.y));
    let (x1, y1) = (to_px(bounds.max.x), to_px(bounds.max.y));

    // A dashed rectangle, so it reads as a selection rather than as ink the
    // user drew.
    let dash = 6.0;
    let mut dashes = |from: (f64, f64), to: (f64, f64)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt().max(1.0);
        let steps = (length / dash).ceil() as i64;
        for step in (0..steps).step_by(2) {
            let t0 = f64::from(step as i32) * dash / length;
            let t1 = ((f64::from(step as i32) + 1.0) * dash / length).min(1.0);
            canvas.stroke_path(
                &[
                    (from.0 + dx * t0, from.1 + dy * t0),
                    (from.0 + dx * t1, from.1 + dy * t1),
                ],
                1.5,
                marker,
            );
        }
    };
    dashes((x0, y0), (x1, y0));
    dashes((x1, y0), (x1, y1));
    dashes((x1, y1), (x0, y1));
    dashes((x0, y1), (x0, y0));

    for (_, handle) in controller.selection_handles() {
        let hx = to_px(handle.min.x).round() as i64;
        let hy = to_px(handle.min.y).round() as i64;
        let size = (to_px(handle.max.x) - to_px(handle.min.x)).round() as i64;
        canvas.fill_rect(hx, hy, size, size, marker);
    }
}

/// Draws the toolbar and its icons.
///
/// Chrome, not document content: it is painted after the scene, never stored,
/// and an ink-only export must exclude it (FR-024).
///
/// Icons are drawn as paths rather than glyphs from a font, because there is no
/// text stack here and a toolbar does not need one. Each icon is described in a
/// unit square and scaled into its button, so the layout in `ink-app` stays the
/// only place that knows about sizes.
pub fn paint_toolbar(
    canvas: &mut Canvas,
    toolbar: &Toolbar,
    tool: Tool,
    colour: Rgb,
    scale: Scale,
) {
    // Premultiplied, memory order B, G, R, A.
    let panel = [0x1E, 0x1A, 0x18, 0xD8];
    let edge = [0x50, 0x48, 0x44, 0xE0];
    let ink = [0xF0, 0xF0, 0xF0, 0xFF];
    let selected_fill = [0x80, 0x70, 0x20, 0xE0];

    let to_px = |value: f64| (value * scale.get()).round() as i64;
    let bounds = toolbar.bounds();
    let (x0, y0) = (to_px(bounds.min.x), to_px(bounds.min.y));
    let (width, height) = (to_px(bounds.max.x) - x0, to_px(bounds.max.y) - y0);

    canvas.fill_rect(x0, y0, width, height, panel);
    canvas.fill_rect(x0, y0, width, 1, edge);
    canvas.fill_rect(x0, y0 + height - 1, width, 1, edge);
    canvas.fill_rect(x0, y0, 1, height, edge);
    canvas.fill_rect(x0 + width - 1, y0, 1, height, edge);

    // The grip: three ridges, the usual shorthand for "drag me".
    let grip = toolbar.grip();
    let gx = to_px(grip.min.x);
    let gy = to_px(grip.min.y);
    let gh = to_px(grip.max.y) - gy;
    let gw = to_px(grip.max.x) - gx;
    for ridge in 0..3 {
        let x = gx + gw / 2 - 4 + i64::from(ridge) * 4;
        canvas.fill_rect(x, gy + gh / 4, 2, gh / 2, edge);
    }

    for button in toolbar.buttons() {
        let bx = to_px(button.bounds.min.x);
        let by = to_px(button.bounds.min.y);
        let size = to_px(button.bounds.max.x) - bx;

        if button.is_selected(tool) {
            canvas.fill_rect(bx, by, size, size, selected_fill);
            // A mark as well as a fill: NFR-006 forbids a selected state
            // carried by colour alone, which also matters on a background
            // that happens to be the same colour.
            canvas.fill_rect(bx, by + size - 2, size, 2, ink);
        }

        // Icons are described in a unit square with a margin, so they never
        // touch the button's edge.
        let inset = 0.24;
        let place = |u: f64, v: f64| {
            (
                (bx as f64) + (inset + u * (1.0 - inset * 2.0)) * size as f64,
                (by as f64) + (inset + v * (1.0 - inset * 2.0)) * size as f64,
            )
        };
        let thickness = (size as f64 * 0.09).max(1.4);
        let mut draw = |path: &[(f64, f64)]| {
            let points: Vec<(f64, f64)> = path.iter().map(|(u, v)| place(*u, *v)).collect();
            canvas.stroke_path(&points, thickness, ink);
        };

        // The colour button shows the colour instead of an icon: it is the
        // clearest possible answer to what you are about to draw with.
        if button.icon == Icon::Color {
            let inset = to_px(4.0);
            canvas.fill_rect(
                bx + inset,
                by + inset,
                size - inset * 2,
                size - inset * 2,
                [colour.b, colour.g, colour.r, 0xFF],
            );
            continue;
        }

        match button.icon {
            // Drawn above as a filled swatch, never as a glyph.
            Icon::Color => {}
            // A capital I with serifs: the usual text cursor.
            Icon::Text => {
                draw(&[(0.5, 0.0), (0.5, 1.0)]);
                draw(&[(0.2, 0.0), (0.8, 0.0)]);
                draw(&[(0.2, 1.0), (0.8, 1.0)]);
            }
            // A box with an arrow pointing into it.
            Icon::Park => {
                draw(&[(0.0, 0.55), (0.0, 1.0), (1.0, 1.0), (1.0, 0.55)]);
                draw(&[(0.5, 0.0), (0.5, 0.62)]);
                draw(&[(0.28, 0.4), (0.5, 0.62), (0.72, 0.4)]);
            }
            // A door with an arrow leaving through it.
            Icon::Quit => {
                draw(&[(0.55, 0.0), (0.0, 0.0), (0.0, 1.0), (0.55, 1.0)]);
                draw(&[(0.35, 0.5), (1.0, 0.5)]);
                draw(&[(0.75, 0.28), (1.0, 0.5), (0.75, 0.72)]);
            }
            // A window with a pin through it.
            Icon::WindowMenu => {
                draw(&[
                    (0.0, 0.25),
                    (1.0, 0.25),
                    (1.0, 1.0),
                    (0.0, 1.0),
                    (0.0, 0.25),
                ]);
                draw(&[(0.5, 0.0), (0.5, 0.45)]);
                draw(&[(0.3, 0.12), (0.7, 0.12)]);
            }
            // An arrow cursor.
            Icon::Select => {
                draw(&[(0.1, 0.0), (0.1, 0.9), (0.38, 0.62), (0.62, 1.0)]);
                draw(&[(0.1, 0.0), (0.72, 0.52), (0.38, 0.62)]);
            }
            // A bin.
            Icon::Delete => {
                draw(&[(0.1, 0.2), (0.9, 0.2)]);
                draw(&[(0.38, 0.2), (0.42, 0.05), (0.58, 0.05), (0.62, 0.2)]);
                draw(&[(0.2, 0.2), (0.28, 1.0), (0.72, 1.0), (0.8, 0.2)]);
            }
            // A nib: a stroke with a tail.
            Icon::Pen => draw(&[(0.0, 1.0), (0.35, 0.55), (1.0, 0.0)]),
            // The same line, deliberately blunter.
            Icon::Highlighter => {
                let points: Vec<(f64, f64)> = [(0.0, 1.0), (1.0, 0.0)]
                    .iter()
                    .map(|(u, v)| place(*u, *v))
                    .collect();
                canvas.stroke_path(&points, thickness * 2.6, ink);
            }
            Icon::Line => draw(&[(0.0, 1.0), (1.0, 0.0)]),
            Icon::Arrow => {
                draw(&[(0.0, 1.0), (1.0, 0.0)]);
                draw(&[(0.45, 0.0), (1.0, 0.0), (1.0, 0.55)]);
            }
            Icon::Rectangle => draw(&[
                (0.0, 0.15),
                (1.0, 0.15),
                (1.0, 0.85),
                (0.0, 0.85),
                (0.0, 0.15),
            ]),
            Icon::Ellipse => {
                let steps = 20;
                let ring: Vec<(f64, f64)> = (0..=steps)
                    .map(|step| {
                        let angle = std::f64::consts::TAU * f64::from(step) / f64::from(steps);
                        (0.5 + 0.5 * angle.cos(), 0.5 + 0.42 * angle.sin())
                    })
                    .collect();
                draw(&ring);
            }
            // A wedge rubbing something out.
            Icon::Eraser => {
                draw(&[
                    (0.0, 0.85),
                    (0.55, 0.15),
                    (1.0, 0.5),
                    (0.45, 1.0),
                    (0.0, 0.85),
                ]);
                draw(&[(0.45, 1.0), (1.0, 1.0)]);
            }
            // An arrow curling back on itself.
            Icon::Undo => {
                draw(&[(1.0, 0.85), (0.45, 0.85), (0.1, 0.5), (0.45, 0.15)]);
                draw(&[(0.1, 0.5), (0.45, 0.5)]);
            }
            Icon::Redo => {
                draw(&[(0.0, 0.85), (0.55, 0.85), (0.9, 0.5), (0.55, 0.15)]);
                draw(&[(0.9, 0.5), (0.55, 0.5)]);
            }
            // A cross: everything goes.
            Icon::Clear => {
                draw(&[(0.0, 0.0), (1.0, 1.0)]);
                draw(&[(1.0, 0.0), (0.0, 1.0)]);
            }
            // An arrow going through a surface.
            Icon::PassThrough => {
                draw(&[(0.5, 0.0), (0.5, 0.8)]);
                draw(&[(0.2, 0.5), (0.5, 0.85), (0.8, 0.5)]);
                draw(&[(0.0, 1.0), (1.0, 1.0)]);
            }
            // A closed eye.
            Icon::Hide => {
                draw(&[(0.0, 0.35), (0.5, 0.8), (1.0, 0.35)]);
                draw(&[(0.15, 0.7), (0.05, 0.95)]);
                draw(&[(0.85, 0.7), (0.95, 0.95)]);
            }
        }
    }
}

/// Draws a caret where a text click would land.
///
/// The system cursor already becomes an I-beam over the canvas, but only once
/// the pointer has moved: selecting the text tool and clicking straight away
/// gives the compositor no motion event to hang a new cursor on, so that first
/// click happens under the old pointer. This is drawn by us, from state we
/// already have, so it is correct the moment the tool changes.
///
/// It is also a better answer to the question than a cursor is. An I-beam says
/// "text goes somewhere near here"; a caret at the exact height of the text
/// says where the line will sit.
pub fn paint_caret_hint(
    canvas: &mut Canvas,
    controller: &Controller,
    pointer: Option<LogicalPoint>,
    scale: Scale,
) {
    // Only while the tool is armed and nothing is being typed yet: once an
    // editor is open its own caret is in the preview, and two would be one too
    // many.
    if controller.tool() != Tool::Text
        || controller.is_editing_text()
        || !Toolbar::canvas_is_interactive(controller.mode())
    {
        return;
    }
    let Some(at) = pointer else { return };
    // Not over the chrome: the caret would be promising text where a button is.
    if controller.toolbar().contains(at)
        || controller.swatches().iter().any(|(_, _, rect)| {
            at.x >= rect.min.x && at.x <= rect.max.x && at.y >= rect.min.y && at.y <= rect.max.y
        })
    {
        return;
    }

    let style = controller.style();
    let alpha = 0.6;
    let channel = |value: u8| (f64::from(value) * alpha).round() as u8;
    let ink = [
        channel(style.color.b),
        channel(style.color.g),
        channel(style.color.r),
        (alpha * 255.0).round() as u8,
    ];

    // The height a line of text would occupy, so the caret shows the size as
    // well as the position.
    let height = style.width.get() * 2.5 * scale.get();
    let x = (at.x * scale.get()).round() as i64;
    let y = (at.y * scale.get()).round() as i64;
    let thickness = (scale.get().round() as i64).max(1);
    let serif = (height / 6.0).round() as i64;

    canvas.fill_rect(x, y, thickness, height.round() as i64, ink);
    canvas.fill_rect(x - serif, y, serif * 2 + thickness, thickness, ink);
    canvas.fill_rect(
        x - serif,
        y + height.round() as i64 - thickness,
        serif * 2 + thickness,
        thickness,
        ink,
    );
}

/// Draws the colour swatch row, when it is open.
///
/// Chrome. The row sits under the colour button, and the current colour gets a
/// mark as well as a ring: NFR-006 forbids a selected state carried by colour
/// alone, which matters more here than anywhere, since every swatch differs
/// only by colour.
pub fn paint_swatches(canvas: &mut Canvas, controller: &Controller, scale: Scale) {
    let swatches = controller.swatches();
    if swatches.is_empty() {
        return;
    }
    let panel = [0x1E, 0x1A, 0x18, 0xD8];
    let edge = [0x50, 0x48, 0x44, 0xE0];
    let mark = [0xF0, 0xF0, 0xF0, 0xFF];

    let to_px = |value: f64| (value * scale.get()).round() as i64;
    let row = controller.toolbar().swatch_row();
    let (x0, y0) = (to_px(row.min.x), to_px(row.min.y));
    let (width, height) = (to_px(row.max.x) - x0, to_px(row.max.y) - y0);
    canvas.fill_rect(x0, y0, width, height, panel);
    canvas.fill_rect(x0, y0, width, 1, edge);
    canvas.fill_rect(x0, y0 + height - 1, width, 1, edge);
    canvas.fill_rect(x0, y0, 1, height, edge);
    canvas.fill_rect(x0 + width - 1, y0, 1, height, edge);

    let current = controller.style().color;
    for (_, colour, rect) in swatches {
        let sx = to_px(rect.min.x);
        let sy = to_px(rect.min.y);
        let size = to_px(rect.max.x) - sx;
        // Swatches are shown at full strength whatever the tool's opacity, so
        // the row is about hue rather than about the current alpha.
        canvas.fill_rect(sx, sy, size, size, [colour.b, colour.g, colour.r, 0xFF]);

        if colour == current {
            canvas.fill_rect(sx - 2, sy - 2, size + 4, 2, mark);
            canvas.fill_rect(sx - 2, sy + size, size + 4, 2, mark);
            canvas.fill_rect(sx - 2, sy - 2, 2, size + 4, mark);
            canvas.fill_rect(sx + size, sy - 2, 2, size + 4, mark);
        }
    }
}

/// Draws a tooltip under the hovered button.
///
/// Chrome. The label font is a 5x7 bitmap with no lowercase, which is why the
/// text is capitals: it exists so the toolbar can have words without a font
/// stack, and the real one arrives with the text tool.
pub fn paint_tooltip(canvas: &mut Canvas, button: &Button, toolbar: LogicalRect, scale: Scale) {
    let background = [0x14, 0x10, 0x0E, 0xE8];
    let border = [0x50, 0x48, 0x44, 0xE0];
    let text = [0xF0, 0xF0, 0xF0, 0xFF];

    let label = button.label();
    let pixel = ((scale.get() * 1.0).round() as usize).max(1);
    let padding = (6.0 * scale.get()).round() as i64;
    let text_width = ink_render::font::text_width(label, pixel) as i64;
    let text_height = ink_render::font::text_height(pixel) as i64;

    let width = text_width + padding * 2;
    let height = text_height + padding * 2;

    // Under the toolbar, aligned to the button, and nudged back inside the
    // surface if the rightmost buttons would push it off the edge.
    let mut x = (button.bounds.min.x * scale.get()).round() as i64;
    let y = (toolbar.max.y * scale.get()).round() as i64 + 6;
    let limit = i64::from(canvas.width()) - width - 4;
    if x > limit {
        x = limit.max(4);
    }

    canvas.fill_rect(x, y, width, height, background);
    canvas.fill_rect(x, y, width, 1, border);
    canvas.fill_rect(x, y + height - 1, width, 1, border);
    canvas.fill_rect(x, y, 1, height, border);
    canvas.fill_rect(x + width - 1, y, 1, height, border);

    canvas.draw_text(label, (x + padding, y + padding), pixel, text);
}

/// Draws the resize grab area as a corner of diagonal ridges.
///
/// Chrome, like the toolbar: it exists so the grab area can be found, since an
/// invisible one is indistinguishable from a bug.
pub fn paint_resize_corner(canvas: &mut Canvas, corner: LogicalRect, scale: Scale) {
    let ink = [0x90, 0x90, 0x90, 0xB0];
    let to_px = |value: f64| value * scale.get();
    let (x1, y1) = (to_px(corner.max.x) - 3.0, to_px(corner.max.y) - 3.0);
    let size = to_px(corner.max.x - corner.min.x) - 6.0;

    for ridge in 1..=3 {
        let offset = size * f64::from(ridge) / 3.5;
        canvas.stroke_path(&[(x1 - offset, y1), (x1, y1 - offset)], 1.6, ink);
    }
}
