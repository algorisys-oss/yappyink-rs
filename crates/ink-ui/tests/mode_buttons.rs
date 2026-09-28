//! The mode in effect, shown on the toolbar.
//!
//! Requirements: FR-030 (the Pass through and Shrink to toolbar buttons show
//! their mode), NFR-006 (not by colour alone). Scenario: AC-FR-030.
//!
//! These prove what the shared painter draws, on whichever OS runs them. They
//! do not prove that a window shows it: that is T041, by eye, per platform.
//!
//! Set `YAPPYINK_RENDER_DIR` to also write the toolbar in each mode as a PPM
//! image, for a person to look at.

use ink_app::keymap::{self, Key};
use ink_app::{Action, Controller, Effect, Icon, Mode, PlatformEvent, Tool};
use ink_core::Rgb;
use ink_render::{Canvas, Scale};
use ink_ui::{PARKED_ACTIVE, PASS_THROUGH_ACTIVE, SELECTED_TOOL};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 60;

/// The icon colour, as drawn on every button.
const ICON: Rgb = Rgb::new(0xF0, 0xF0, 0xF0);

fn settle(controller: &mut Controller, action: Action) {
    let effects = controller.act(action);
    if let Some(Effect::ApplyMode { transition, .. }) = effects
        .iter()
        .find(|e| matches!(e, Effect::ApplyMode { .. }))
    {
        let transition = *transition;
        controller.handle(PlatformEvent::ModeApplied { transition });
    }
}

fn in_mode(mode: Mode) -> Controller {
    let mut controller = Controller::new();
    settle(&mut controller, Action::EnterDraw);
    match mode {
        Mode::Draw => {}
        Mode::PassThrough => settle(&mut controller, Action::ToggleDraw),
        Mode::Parked => settle(&mut controller, Action::TogglePark),
        Mode::Hidden => settle(&mut controller, Action::ToggleVisibility),
    }
    assert_eq!(controller.mode(), mode, "could not reach {mode:?}");
    controller
}

/// The toolbar as the backends paint it, from the controller's own state.
fn toolbar_pixels(controller: &Controller) -> Vec<u8> {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let mut canvas = Canvas::new(&mut pixels, WIDTH, HEIGHT).unwrap();
    ink_ui::paint_toolbar(
        &mut canvas,
        controller.toolbar(),
        controller.tool(),
        controller.style().color,
        controller.mode(),
        Scale::ONE,
    );
    pixels
}

fn pixel(pixels: &[u8], x: i64, y: i64) -> [u8; 4] {
    let at = ((y as u32 * WIDTH + x as u32) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

/// Memory order B, G, R, A, opaque.
fn bgra(colour: Rgb) -> [u8; 4] {
    [colour.b, colour.g, colour.r, 0xFF]
}

/// The button's rectangle in pixels: x, y, edge.
fn button(controller: &Controller, icon: Icon) -> (i64, i64, i64) {
    let b = controller
        .toolbar()
        .buttons()
        .iter()
        .find(|b| b.icon == icon)
        .unwrap_or_else(|| panic!("no {icon:?} button"));
    let x = b.bounds.min.x.round() as i64;
    let y = b.bounds.min.y.round() as i64;
    (x, y, (b.bounds.max.x - b.bounds.min.x).round() as i64)
}

/// Inside the button, clear of the outline and of the icon: where the fill
/// shows.
fn fill_of(pixels: &[u8], (x, y, _): (i64, i64, i64)) -> [u8; 4] {
    pixel(pixels, x + 4, y + 4)
}

/// On the button's left edge, halfway down: where an outline would be.
fn edge_of(pixels: &[u8], (x, y, size): (i64, i64, i64)) -> [u8; 4] {
    pixel(pixels, x, y + size / 2)
}

const MODES: [Mode; 3] = [Mode::Draw, Mode::PassThrough, Mode::Parked];

#[test]
fn the_toolbar_looks_different_in_each_mode() {
    let seen: Vec<(Mode, Vec<u8>)> = MODES
        .iter()
        .map(|mode| (*mode, toolbar_pixels(&in_mode(*mode))))
        .collect();

    for (i, (a_mode, a)) in seen.iter().enumerate() {
        for (b_mode, b) in seen.iter().skip(i + 1) {
            assert_ne!(a, b, "the toolbar is the same in {a_mode:?} and {b_mode:?}");
        }
    }
}

#[test]
fn the_pass_through_button_is_coloured_only_in_pass_through() {
    for mode in MODES {
        let controller = in_mode(mode);
        let pixels = toolbar_pixels(&controller);
        let fill = fill_of(&pixels, button(&controller, Icon::PassThrough));
        assert_eq!(
            fill == bgra(PASS_THROUGH_ACTIVE),
            mode == Mode::PassThrough,
            "Pass through button fill in {mode:?}: {fill:?}"
        );
    }
}

#[test]
fn the_shrink_button_is_coloured_only_while_parked() {
    for mode in MODES {
        let controller = in_mode(mode);
        let pixels = toolbar_pixels(&controller);
        let fill = fill_of(&pixels, button(&controller, Icon::Park));
        assert_eq!(
            fill == bgra(PARKED_ACTIVE),
            mode == Mode::Parked,
            "Shrink button fill in {mode:?}: {fill:?}"
        );
    }
}

/// NFR-006: someone who cannot tell the fill from the panel still sees an
/// outline.
#[test]
fn an_active_mode_button_is_marked_as_well_as_coloured() {
    for mode in MODES {
        let controller = in_mode(mode);
        let pixels = toolbar_pixels(&controller);
        for (icon, active_in) in [
            (Icon::PassThrough, Mode::PassThrough),
            (Icon::Park, Mode::Parked),
        ] {
            let edge = edge_of(&pixels, button(&controller, icon));
            assert_eq!(
                edge == bgra(ICON),
                mode == active_in,
                "{icon:?} outline in {mode:?}: {edge:?}"
            );
        }
    }
}

/// The two indications are separate: the mode does not move or remove the
/// tool highlight.
#[test]
fn the_selected_tool_keeps_its_highlight_in_every_mode() {
    // The highlight is blended over the panel, so compare with Draw rather
    // than with the constant.
    let draw = in_mode(Mode::Draw);
    let draw_pixels = toolbar_pixels(&draw);
    let highlighted = fill_of(&draw_pixels, button(&draw, Icon::Pen));
    assert_eq!(draw.tool(), Tool::Pen);
    assert_ne!(
        highlighted,
        fill_of(&draw_pixels, button(&draw, Icon::Line)),
        "the pen is not highlighted even in Draw"
    );

    for mode in MODES {
        let controller = in_mode(mode);
        let pixels = toolbar_pixels(&controller);
        assert_eq!(
            fill_of(&pixels, button(&controller, Icon::Pen)),
            highlighted,
            "the pen lost its highlight in {mode:?}"
        );
    }
}

/// The toolbar never claims a mode the platform has not confirmed.
#[test]
fn a_requested_mode_is_not_shown_until_it_is_applied() {
    let mut controller = in_mode(Mode::Draw);
    let effects = controller.act(Action::ToggleDraw);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::ApplyMode { .. }))
    );
    assert_eq!(controller.desired_mode(), Mode::PassThrough);

    let pixels = toolbar_pixels(&controller);
    let fill = fill_of(&pixels, button(&controller, Icon::PassThrough));
    assert_ne!(fill, bgra(PASS_THROUGH_ACTIVE));
}

/// Every key into and out of the two modes ends with the toolbar showing the
/// mode that is now in effect. The chords and commands reach the controller
/// as the same actions, so they are covered by the actions below.
#[test]
fn every_route_leaves_the_toolbar_showing_the_mode_in_effect() {
    let key = |c| keymap::command(Key::Char(c)).expect("a bound key");
    let routes: Vec<(&str, Action, Mode)> = vec![
        ("p", key('p'), Mode::PassThrough),
        ("d", key('d'), Mode::Draw),
        ("g", key('g'), Mode::Parked),
        ("g again", key('g'), Mode::Draw),
        ("p", key('p'), Mode::PassThrough),
        ("g from pass-through", key('g'), Mode::Parked),
        ("d", key('d'), Mode::Draw),
        ("p", key('p'), Mode::PassThrough),
        ("p again", key('p'), Mode::Draw),
        (
            "Esc",
            keymap::command(Key::Escape).unwrap(),
            Mode::PassThrough,
        ),
        ("hide", Action::ToggleVisibility, Mode::Hidden),
        ("show", Action::ToggleVisibility, Mode::PassThrough),
        ("toggle-draw", Action::ToggleDraw, Mode::Draw),
    ];

    let mut controller = in_mode(Mode::Draw);
    for (name, action, expected) in routes {
        settle(&mut controller, action);
        assert_eq!(controller.mode(), expected, "after {name}");
        if expected == Mode::Hidden {
            continue;
        }
        let pixels = toolbar_pixels(&controller);
        let pass = fill_of(&pixels, button(&controller, Icon::PassThrough));
        let park = fill_of(&pixels, button(&controller, Icon::Park));
        assert_eq!(
            pass == bgra(PASS_THROUGH_ACTIVE),
            expected == Mode::PassThrough,
            "Pass through button after {name}"
        );
        assert_eq!(
            park == bgra(PARKED_ACTIVE),
            expected == Mode::Parked,
            "Shrink button after {name}"
        );
    }
}

#[test]
fn the_tooltip_says_when_a_mode_is_on() {
    let controller = in_mode(Mode::PassThrough);
    let find = |icon| {
        *controller
            .toolbar()
            .buttons()
            .iter()
            .find(|b| b.icon == icon)
            .unwrap()
    };
    let pass = find(Icon::PassThrough);
    let park = find(Icon::Park);

    assert_eq!(pass.label_in(Mode::PassThrough), "PASS THROUGH: ON (P)");
    assert_eq!(pass.label_in(Mode::Draw), pass.label());
    assert_eq!(park.label_in(Mode::Parked), "SHRINK TO TOOLBAR: ON (G)");
    assert_eq!(park.label_in(Mode::PassThrough), park.label());
}

/// WCAG relative luminance, for the contrast check below.
fn luminance(colour: Rgb) -> f64 {
    let channel = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(colour.r) + 0.7152 * channel(colour.g) + 0.0722 * channel(colour.b)
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn the_icon_stays_readable_on_both_fills() {
    for (name, fill) in [
        ("pass-through", PASS_THROUGH_ACTIVE),
        ("parked", PARKED_ACTIVE),
    ] {
        let ratio = contrast(ICON, fill);
        assert!(ratio >= 3.0, "icon on the {name} fill is {ratio:.2}:1");
    }
}

#[test]
fn the_three_highlights_are_different_colours() {
    let colours = [PASS_THROUGH_ACTIVE, PARKED_ACTIVE, SELECTED_TOOL];
    for (i, a) in colours.iter().enumerate() {
        for b in colours.iter().skip(i + 1) {
            let distance = (i32::from(a.r) - i32::from(b.r)).abs()
                + (i32::from(a.g) - i32::from(b.g)).abs()
                + (i32::from(a.b) - i32::from(b.b)).abs();
            assert!(distance > 150, "{a:?} and {b:?} are too alike");
        }
    }
}

/// Writes the toolbar in each mode for review when `YAPPYINK_RENDER_DIR` is
/// set. Always passes: it produces pictures, it does not judge them.
#[test]
fn render_for_review() {
    let Some(dir) = std::env::var_os("YAPPYINK_RENDER_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (mode, name) in [
        (Mode::Draw, "draw"),
        (Mode::PassThrough, "pass-through"),
        (Mode::Parked, "parked"),
    ] {
        let controller = in_mode(mode);
        let pixels = toolbar_pixels(&controller);
        let right = (controller.toolbar().bounds().max.x.round() as u32 + 10).min(WIDTH);
        // Composited over a mid-grey desktop, un-premultiplied, as RGB.
        let mut out = format!("P6\n{right} {HEIGHT}\n255\n").into_bytes();
        for y in 0..HEIGHT {
            for x in 0..right {
                let [b, g, r, a] = pixel(&pixels, i64::from(x), i64::from(y));
                let over =
                    |c: u8| (u32::from(c) + 0x80 * (255 - u32::from(a)) / 255).min(255) as u8;
                out.extend([over(r), over(g), over(b)]);
            }
        }
        std::fs::write(dir.join(format!("toolbar-{name}.ppm")), out).unwrap();
    }
}
