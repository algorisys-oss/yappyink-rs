//! Frame cost: one pointer move plus a full repaint, the way every adapter
//! does it, over scenes from empty to the document limit (T024, NFR-001).
//!
//! Ignored by default because it is a measurement, not a check, and takes
//! minutes. Results and their conditions are recorded in
//! `docs/evidence/E015-performance-on-the-e001-machine.md`. Run with:
//!
//! ```sh
//! cargo test --release -p ink-ui --test frame_cost -- --ignored --nocapture --test-threads=1
//! ```
//!
//! This times submission work on the CPU. It says nothing about the time from
//! a pointer event to photons, which needs a camera.

use std::time::Instant;

use ink_app::{Action, Controller, Effect, PlatformEvent, Tool};
use ink_core::{
    LogicalPoint, LogicalSize, Object, ObjectId, OutputId, Session, Shape, StrokeKind, Style,
};
use ink_render::text::TextFont;
use ink_render::{Canvas, Painter, Scale};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn p(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).unwrap()
}

fn drawing() -> Controller {
    let mut c = Controller::new();
    let t = c
        .act(Action::EnterDraw)
        .iter()
        .find_map(|e| match e {
            Effect::ApplyMode { transition, .. } => Some(*transition),
            _ => None,
        })
        .unwrap();
    c.handle(PlatformEvent::ModeApplied { transition: t });
    c.act(Action::SelectTool(Tool::Pen));
    c
}

fn scene(strokes: usize, w: f64, h: f64, style: Style) -> Session {
    let out = OutputId::new("bench");
    let mut s = Session::new(out.clone(), LogicalSize::new(w, h).unwrap());
    let mut r = Rng(7);
    for i in 0..strokes {
        let (mut x, mut y) = (r.next() * w, r.next() * h);
        let mut pts = Vec::with_capacity(60);
        for _ in 0..60 {
            x = (x + (r.next() - 0.5) * 12.0).clamp(0.0, w);
            y = (y + (r.next() - 0.5) * 12.0).clamp(0.0, h);
            pts.push(p(x, y));
        }
        let kind = if i % 5 == 4 {
            StrokeKind::Highlighter
        } else {
            StrokeKind::Pen
        };
        let shape = Shape::stroke(kind, pts).unwrap();
        s.add(Object::new(
            ObjectId::from_raw(i as u64 + 1),
            out.clone(),
            style,
            shape,
        ))
        .unwrap();
    }
    s
}

fn frame(
    canvas: &mut Canvas,
    ink: &mut ink_ui::InkLayer,
    c: &Controller,
    s: &Session,
    painter: &mut Painter,
    scale: Scale,
) {
    ink_ui::clear(canvas, true);
    ink.paint(canvas, c, s.document(), painter, scale);
    ink_ui::paint_preview(canvas, c, s.document().output(), painter, scale);
    ink_ui::paint_chrome(canvas, c.mode(), c.style(), c.tool());
    ink_ui::paint_selection(canvas, c, s.document(), painter, scale);
    ink_ui::paint_toolbar(canvas, c.toolbar(), c.tool(), c.style().color, scale);
}

#[test]
#[ignore]
fn frame_cost() {
    for (label, lw, lh, factor) in [
        ("1280x720 @1x (Linux default window)", 1280.0, 720.0, 1.0),
        ("1920x1080 @1x (full HD monitor)", 1920.0, 1080.0, 1.0),
        ("1440x900 @2x (Retina, 2880x1800 px)", 1440.0, 900.0, 2.0),
    ] {
        let (pw, ph) = ((lw * factor) as u32, (lh * factor) as u32);
        let scale = Scale::new(factor).unwrap();
        for (name, strokes, iters) in [
            ("empty", 0usize, 300usize),
            ("normal, 100 strokes", 100, 300),
            ("stress, 1000 strokes", 1000, 100),
            ("limit, 10000 strokes", 10_000, 20),
        ] {
            let mut ctl = drawing();
            let session = scene(strokes, lw, lh, ctl.style());
            let mut painter = match TextFont::discover() {
                Ok(f) => Painter::new().with_font(f),
                Err(_) => Painter::new(),
            };
            let mut px = vec![0u8; (pw * ph * 4) as usize];
            let mut canvas = Canvas::new(&mut px, pw, ph).unwrap();
            let mut r = Rng(11);
            let (mut x, mut y) = (lw / 2.0, lh / 2.0);
            ctl.handle(PlatformEvent::PointerDown { at: p(x, y) });
            // Warm up, which also builds the ink layer. The timed frames are
            // the steady state while drawing; a rebuild happens once per
            // commit, and costs what a frame used to (see `breakdown`).
            let mut ink = ink_ui::InkLayer::new();
            frame(&mut canvas, &mut ink, &ctl, &session, &mut painter, scale);
            let mut times = Vec::with_capacity(iters);
            for i in 0..iters {
                if i % 150 == 149 {
                    ctl.handle(PlatformEvent::PointerUp { at: p(x, y) });
                    ctl.handle(PlatformEvent::PointerDown { at: p(x, y) });
                }
                x = (x + (r.next() - 0.5) * 16.0).clamp(1.0, lw - 1.0);
                y = (y + (r.next() - 0.5) * 16.0).clamp(80.0, lh - 1.0);
                let start = Instant::now();
                ctl.handle(PlatformEvent::PointerMoved { at: p(x, y) });
                frame(&mut canvas, &mut ink, &ctl, &session, &mut painter, scale);
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let q = |f: f64| times[((times.len() - 1) as f64 * f).round() as usize];
            println!(
                "PERF | {label} | {name} | n={} | p50 {:.2} ms | p95 {:.2} ms | p99 {:.2} ms | max {:.2} ms",
                times.len(),
                q(0.5),
                q(0.95),
                q(0.99),
                times[times.len() - 1]
            );
        }
    }
}

#[test]
#[ignore]
fn breakdown() {
    let (lw, lh, factor) = (1920.0, 1080.0, 1.0);
    let (pw, ph) = (1920u32, 1080u32);
    let scale = Scale::new(factor).unwrap();
    let mut ctl = drawing();
    let session = scene(1000, lw, lh, ctl.style());
    ctl.handle(PlatformEvent::PointerDown {
        at: p(500.0, 500.0),
    });
    for i in 0..100 {
        ctl.handle(PlatformEvent::PointerMoved {
            at: p(500.0 + i as f64 * 3.0, 500.0 + i as f64),
        });
    }
    let mut painter = Painter::new();
    let mut px = vec![0u8; (pw * ph * 4) as usize];
    let mut canvas = Canvas::new(&mut px, pw, ph).unwrap();
    let time = |label: &str, f: &mut dyn FnMut()| {
        f();
        let n = 20;
        let start = Instant::now();
        for _ in 0..n {
            f();
        }
        println!(
            "PART | {label} | {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    };
    time("clear with floor", &mut || ink_ui::clear(&mut canvas, true));
    time("document, 1000 strokes", &mut || {
        ink_ui::paint_document(&mut canvas, &ctl, session.document(), &mut painter, scale)
    });
    time("preview, 100-point stroke", &mut || {
        ink_ui::paint_preview(
            &mut canvas,
            &ctl,
            session.document().output(),
            &mut painter,
            scale,
        )
    });
    time("chrome (frame, badge)", &mut || {
        ink_ui::paint_chrome(&mut canvas, ctl.mode(), ctl.style(), ctl.tool())
    });
    time("toolbar", &mut || {
        ink_ui::paint_toolbar(
            &mut canvas,
            ctl.toolbar(),
            ctl.tool(),
            ctl.style().color,
            scale,
        )
    });
    let one = session.document().objects().next().unwrap();
    time("one 60-point stroke", &mut || {
        painter.paint_object(one, &mut canvas, scale)
    });
    println!("PART | stroke width {} logical", ctl.style().width.get());
}
