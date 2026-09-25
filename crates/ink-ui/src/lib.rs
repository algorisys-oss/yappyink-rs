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

use ink_app::{Button, Controller, Icon, Mode, Preview, Tool, Toolbar};
use ink_core::{
    Document, LogicalPoint, LogicalRect, Object, ObjectId, Opacity, OutputId, Rgb, Shape,
    StrokeKind, Style, Width,
};
use ink_render::{Canvas, Painter, Scale};

/// The faintest pixel a window system still counts as part of the window:
/// black at alpha 1 of 255, premultiplied. Over white it reads as 254, which
/// nobody can see.
pub const CAPTURE_FLOOR: [u8; 4] = [0, 0, 0, 1];

/// Starts a frame: fully transparent, or covered by [`CAPTURE_FLOOR`] where the
/// surface has to take the pointer.
///
/// # Why a backend would want the floor
///
/// Win32 layered windows let mouse input through wherever a pixel's alpha is
/// zero; Microsoft documents it under "Layered Windows". AppKit does the same
/// for a window with a clear background. On both, a canvas cleared to nothing
/// in Draw mode would receive clicks only on the frame, the toolbar and ink
/// already drawn, and every click on empty space would land in the application
/// underneath. AGENTS.md puts it as "do not equate transparent pixels with
/// correct input capture", and this is the same statement the other way round:
/// transparent pixels are not capturing pixels either, unless something makes
/// them so.
///
/// Wayland does not need this. Input there is decided by an explicit input
/// region, independent of what is painted (E003 finding 1), so its adapter
/// keeps clearing to nothing.
///
/// Only the backend knows which kind of window system it is on, so it says
/// whether it is `capturing` rather than this function inferring it from a
/// mode.
pub fn clear(canvas: &mut Canvas, capturing: bool) {
    if capturing {
        canvas.fill(CAPTURE_FLOOR);
    } else {
        canvas.clear();
    }
}

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

/// Draws the document, as the current mode and selection want it seen.
///
/// Nothing at all while the mode hides ink (Parked keeps the document but does
/// not show it). While a selection is being dragged, the selected objects are
/// drawn faded where they still are, and [`paint_selection`] draws them at full
/// strength where they are going; without the fade the two read as a duplicate
/// rather than a move.
///
/// Lifted from the Wayland adapter for the same reason as [`paint_preview`]:
/// the other two painted the whole document unconditionally, so on Windows and
/// macOS a drag showed two identical copies and a parked overlay kept its ink.
pub fn paint_document(
    canvas: &mut Canvas,
    controller: &Controller,
    document: &Document,
    painter: &mut Painter,
    scale: Scale,
) {
    if !Toolbar::ink_is_visible(controller.mode()) {
        return;
    }
    if controller.selection_drag().is_none() {
        painter.paint(document, canvas, scale);
        return;
    }
    let selection = controller.selection();
    for object in document.objects() {
        if selection.contains(&object.id()) {
            if let Some(faded) = faded(object) {
                painter.paint_object(&faded, canvas, scale);
            }
        } else {
            painter.paint_object(object, canvas, scale);
        }
    }
}

/// A raster of the committed document, repainted only when something it
/// depends on changes.
///
/// Every pointer move repaints the frame, and before this every one of them
/// re-rasterised every committed object. That made a frame cost linear in the
/// ink on screen: 116 ms at 1080p with 1,000 strokes, against a 16.7 ms budget,
/// with 101 ms of it ink that had not changed (E015). With the layer, a move
/// costs one composite of the cached pixels plus the stroke in flight.
///
/// The key is everything the pixels depend on: the document's revision, the
/// surface size, the scale, whether the mode shows ink, and which objects are
/// faded because a selection is being dragged. Anything else is drawn fresh
/// every frame, over the layer, by the other functions in this crate.
#[derive(Default)]
pub struct InkLayer {
    pixels: Vec<u8>,
    /// The byte range of each row that holds any ink, found once per rebuild.
    /// Only these are composited, so an empty page costs nothing per frame.
    spans: Vec<std::ops::Range<usize>>,
    key: Option<LayerKey>,
    /// How many times the layer was rebuilt, for tests and diagnostics.
    rebuilds: u64,
}

/// For each row, the byte range from its first inked pixel to its last.
/// Rows with no ink contribute nothing.
fn ink_spans(pixels: &[u8], width: usize) -> Vec<std::ops::Range<usize>> {
    let stride = width * 4;
    if stride == 0 {
        return Vec::new();
    }
    pixels
        .chunks_exact(stride)
        .enumerate()
        .filter_map(|(row, bytes)| {
            let inked = |pixel: &[u8]| pixel[3] != 0;
            let first = bytes.chunks_exact(4).position(inked)?;
            let last = bytes.chunks_exact(4).rposition(inked)?;
            Some(row * stride + first * 4..row * stride + (last + 1) * 4)
        })
        .collect()
}

#[derive(Clone, PartialEq)]
struct LayerKey {
    revision: u64,
    width: u32,
    height: u32,
    scale_bits: u64,
    visible: bool,
    faded: Vec<ink_core::ObjectId>,
}

impl InkLayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many times the document has been re-rasterised.
    pub fn rebuilds(&self) -> u64 {
        self.rebuilds
    }

    /// Paints the document onto `canvas`, rebuilding the cached raster first
    /// only if it is stale. Produces the same pixels as [`paint_document`].
    pub fn paint(
        &mut self,
        canvas: &mut Canvas,
        controller: &Controller,
        document: &Document,
        painter: &mut Painter,
        scale: Scale,
    ) {
        let key = LayerKey {
            revision: document.revision(),
            width: canvas.width(),
            height: canvas.height(),
            scale_bits: scale.get().to_bits(),
            visible: Toolbar::ink_is_visible(controller.mode()),
            faded: if controller.selection_drag().is_some() {
                controller.selection().to_vec()
            } else {
                Vec::new()
            },
        };
        if !key.visible {
            return;
        }
        if self.key.as_ref() != Some(&key) {
            let length = key.width as usize * key.height as usize * 4;
            self.pixels.clear();
            self.pixels.resize(length, 0);
            if let Some(mut layer) = Canvas::new(&mut self.pixels, key.width, key.height) {
                paint_document(&mut layer, controller, document, painter, scale);
            }
            self.spans = ink_spans(&self.pixels, key.width as usize);
            self.key = Some(key);
            self.rebuilds += 1;
        }
        for span in &self.spans {
            canvas.composite_range(&self.pixels, span.clone());
        }
    }
}

/// Draws the gesture in flight: a stroke or shape while it is being dragged,
/// the eraser's sweep, and text while it is being typed.
///
/// None of it is in the document, which is the point of keeping the preview
/// separate from committed state: an abandoned gesture leaves nothing behind.
///
/// This lived in the Wayland adapter until the other two backends needed it,
/// and they did not have it. Both skipped `Preview::Text`, so on Windows and
/// macOS typed text was invisible until Return committed it. That is the
/// "correct in the model, absent on screen" failure `docs/learning.md` §1 is
/// about, found by reading rather than by a user this time.
///
/// The preview is given the largest possible id. It is never stored, so it
/// cannot collide with anything, and drawing it does not spend an id from the
/// document's own sequence on every frame.
pub fn paint_preview(
    canvas: &mut Canvas,
    controller: &Controller,
    output: &OutputId,
    painter: &mut Painter,
    scale: Scale,
) {
    let Some(preview) = controller.preview() else {
        return;
    };
    let mut preedit_underline = None;
    let built = match preview {
        Preview::Stroke {
            points,
            kind,
            style,
        } => Shape::stroke(kind, points.to_vec())
            .ok()
            .map(|shape| (shape, style)),
        Preview::Shape { shape, style } => Some((shape, style)),
        // A caret is appended so an empty editor is still visible: a click that
        // opened one and drew nothing would otherwise look like the tool
        // failing. Composition is drawn inline where it will land, and
        // underlined below so the user can see what is still provisional.
        Preview::Text {
            at,
            content,
            preedit,
            size,
            style,
        } => {
            if !preedit.is_empty() {
                preedit_underline = Some((at, content, preedit, size));
            }
            Shape::text(at, format!("{content}{preedit}|"), size)
                .ok()
                .map(|shape| (shape, style))
        }
        // Faint and at the full width it will clear, so the user can see what
        // it is about to take.
        Preview::Erase { path, radius } => Shape::stroke(StrokeKind::Pen, path.to_vec())
            .ok()
            .and_then(|shape| {
                let width = Width::new(radius * 2.0)?;
                let faint = Opacity::new(0.35)?;
                Some((shape, Style::new(Rgb::new(200, 200, 200), width, faint)))
            }),
    };
    if let Some((shape, style)) = built {
        let object = Object::new(ObjectId::from_raw(u64::MAX), output.clone(), style, shape);
        painter.paint_object(&object, canvas, scale);
    }

    // After the glyphs, so it sits under the characters it marks.
    if let Some((at, content, preedit, size)) = preedit_underline
        && let Some(font) = painter.font()
    {
        let scale = scale.get();
        let pixels = (size * scale) as f32;
        // The last line only: a composition never spans one.
        let before = content.rsplit('\n').next().unwrap_or("");
        let start = (at.x * scale) as f32 + font.line_width(before, pixels);
        let width = font.line_width(preedit, pixels);
        let down = content.matches('\n').count() as f64 * size * 1.25 * scale;
        let baseline = (at.y * scale + down) as f32 + font.ascent(pixels) + 2.0;
        let thickness = (scale.round() as i64).max(1);
        canvas.fill_rect(
            start.round() as i64,
            baseline.round() as i64,
            width.round() as i64,
            thickness,
            [0xD0, 0xD0, 0xD0, 0xD0],
        );
    }
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
            // A pencil lying on the diagonal: the body, the sharpened tip at
            // the lower left, the edge where the wood is cut, and a band near
            // the far end. It used to be a single bent line, which at toolbar
            // size could not be told from the Line tool beside it.
            Icon::Pen => {
                draw(&[
                    (0.0, 1.0),
                    (0.44, 0.86),
                    (1.0, 0.30),
                    (0.70, 0.0),
                    (0.14, 0.56),
                    (0.0, 1.0),
                ]);
                draw(&[(0.44, 0.86), (0.14, 0.56)]);
                draw(&[(0.88, 0.42), (0.58, 0.12)]);
            }
            // A marker: a wide body on the diagonal, a chisel tip cut flat,
            // and the broad stripe it leaves underneath. It used to be a thick
            // bar, which read as a heavy line rather than as a highlighter,
            // and the chisel keeps it distinct from the pencil's point.
            Icon::Highlighter => {
                draw(&[
                    (0.30, 0.44),
                    (0.56, 0.70),
                    (1.0, 0.26),
                    (0.74, 0.0),
                    (0.30, 0.44),
                ]);
                draw(&[(0.30, 0.44), (0.14, 0.62), (0.34, 0.82), (0.56, 0.70)]);
                let stripe: Vec<(f64, f64)> = [(0.0, 0.97), (0.52, 0.97)]
                    .iter()
                    .map(|(u, v)| place(*u, *v))
                    .collect();
                canvas.stroke_path(&stripe, thickness * 2.2, ink);
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
            // An arrow that turns back on itself, the way every editor draws
            // it: a head, a shaft across the top, and a half-turn underneath.
            // The first version was a wedge with a tail, which read as "<"
            // rather than as going back. Redo is the same shape mirrored.
            Icon::Undo | Icon::Redo => {
                let mirror = |u: f64| {
                    if button.icon == Icon::Redo {
                        1.0 - u
                    } else {
                        u
                    }
                };
                let (turn_x, turn_y, radius) = (0.60, 0.61, 0.25);
                let mut path = vec![(mirror(0.06), 0.36)];
                for step in 0..=12 {
                    let angle = -std::f64::consts::FRAC_PI_2
                        + std::f64::consts::PI * f64::from(step) / 12.0;
                    let u = turn_x + radius * angle.cos();
                    path.push((mirror(u), turn_y + radius * angle.sin()));
                }
                path.push((mirror(0.30), turn_y + radius));
                draw(&path);
                draw(&[
                    (mirror(0.28), 0.14),
                    (mirror(0.06), 0.36),
                    (mirror(0.28), 0.58),
                ]);
            }
            // A magnifying glass with a plus in the lens.
            Icon::Zoom => {
                let ring: Vec<(f64, f64)> = (0..=20)
                    .map(|step| {
                        let angle = std::f64::consts::TAU * f64::from(step) / 20.0;
                        (0.40 + 0.34 * angle.cos(), 0.40 + 0.34 * angle.sin())
                    })
                    .collect();
                draw(&ring);
                draw(&[(0.65, 0.65), (1.0, 1.0)]);
                draw(&[(0.24, 0.40), (0.56, 0.40)]);
                draw(&[(0.40, 0.24), (0.40, 0.56)]);
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
