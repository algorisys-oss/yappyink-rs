//! The text tool.
//!
//! Requirement: FR-023 (text and editing), FR-008 (no invisible objects),
//! NFR-003 (bounds). Task: T028, the Latin half.
//!
//! The focus policy is the part worth testing: while the editor is open a key
//! is a character, not a shortcut, and the application has to be able to tell
//! the difference.

use ink_app::{Action, Controller, Effect, PlatformEvent, Preview, Tool};
use ink_core::{LogicalPoint, Shape, limits};

fn point(x: f64, y: f64) -> LogicalPoint {
    LogicalPoint::new(x, y).expect("finite test coordinate")
}

fn typing() -> Controller {
    let mut controller = Controller::new();
    let effects = controller.act(Action::EnterDraw);
    let transition = effects
        .iter()
        .find_map(|e| match e {
            Effect::ApplyMode { transition, .. } => Some(*transition),
            _ => None,
        })
        .expect("EnterDraw requests a mode");
    controller.handle(PlatformEvent::ModeApplied { transition });
    controller.act(Action::SelectTool(Tool::Text));
    controller
}

fn type_word(controller: &mut Controller, word: &str) {
    for character in word.chars() {
        controller.act(Action::TypeText(character));
    }
}

fn committed_text(effects: &[Effect]) -> Option<(String, f64)> {
    effects.iter().find_map(|e| match e {
        Effect::CommitObject {
            shape: Shape::Text { content, size, .. },
            ..
        } => Some((content.clone(), *size)),
        _ => None,
    })
}

// --- Placing and typing ----------------------------------------------------

#[test]
fn the_editor_opens_where_you_click() {
    let mut controller = typing();
    assert!(!controller.is_editing_text());

    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });

    assert!(controller.is_editing_text());
    match controller.preview() {
        Some(Preview::Text { at, .. }) => assert_eq!(at, point(200.0, 300.0)),
        other => panic!("expected a text preview, got {other:?}"),
    }
}

#[test]
fn typed_characters_appear_in_the_preview() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });

    type_word(&mut controller, "Hello");

    match controller.preview() {
        Some(Preview::Text { content, .. }) => assert_eq!(content, "Hello"),
        other => panic!("expected a text preview, got {other:?}"),
    }
}

#[test]
fn backspace_removes_the_last_character() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "Hey");

    controller.act(Action::BackspaceText);

    match controller.preview() {
        Some(Preview::Text { content, .. }) => assert_eq!(content, "He"),
        other => panic!("expected a text preview, got {other:?}"),
    }
}

#[test]
fn a_newline_starts_a_second_line() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "a");
    controller.act(Action::NewlineText);
    type_word(&mut controller, "b");

    let effects = controller.act(Action::CommitText);

    assert_eq!(
        committed_text(&effects).map(|(text, _)| text),
        Some("a\nb".to_owned())
    );
}

#[test]
fn text_is_bounded() {
    // NFR-003. An annotation is a label, not a document.
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });

    for _ in 0..(limits::MAX_TEXT_CHARS + 50) {
        controller.act(Action::TypeText('x'));
    }
    let effects = controller.act(Action::CommitText);

    let (text, _) = committed_text(&effects).expect("a commit");
    assert_eq!(text.chars().count(), limits::MAX_TEXT_CHARS);
}

// --- FR-023: explicit focus ------------------------------------------------

#[test]
fn the_editor_owns_the_keyboard_while_it_is_open() {
    // The adapter asks this before deciding whether a key is a character or a
    // shortcut. Without it, typing "q" would quit and "x" would clear.
    let mut controller = typing();
    assert!(!controller.is_editing_text());

    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    assert!(controller.is_editing_text());

    controller.act(Action::CommitText);
    assert!(
        !controller.is_editing_text(),
        "focus is released on leaving the editor"
    );
}

// --- Committing and abandoning ---------------------------------------------

#[test]
fn committing_keeps_what_was_typed() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "Note");

    let effects = controller.act(Action::CommitText);

    assert_eq!(
        committed_text(&effects).map(|(text, _)| text),
        Some("Note".to_owned())
    );
    assert!(!controller.is_editing_text());
}

#[test]
fn escape_abandons_the_editor_without_committing() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "oops");

    let effects = controller.act(Action::Escape);

    assert!(
        committed_text(&effects).is_none(),
        "Escape kept the text: {effects:?}"
    );
    assert!(!controller.is_editing_text());
}

#[test]
fn escape_closes_the_editor_before_it_touches_the_mode() {
    // Escape undoes one thing at a time, smallest first, and an open editor is
    // the smallest.
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });

    controller.act(Action::Escape);

    assert_eq!(
        controller.mode(),
        ink_app::Mode::Draw,
        "Escape also left Draw"
    );
}

#[test]
fn an_empty_editor_commits_nothing() {
    // FR-008: an invisible object must not be created. Clicking somewhere and
    // changing your mind is not an edit.
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });

    let effects = controller.act(Action::CommitText);

    assert!(effects.is_empty(), "an empty editor produced {effects:?}");
}

#[test]
fn whitespace_alone_commits_nothing() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "   ");

    let effects = controller.act(Action::CommitText);

    assert!(effects.is_empty());
}

#[test]
fn clicking_elsewhere_commits_and_opens_a_new_editor() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "first");

    let effects = controller.handle(PlatformEvent::PointerDown {
        at: point(400.0, 500.0),
    });

    assert_eq!(
        committed_text(&effects).map(|(text, _)| text),
        Some("first".to_owned())
    );
    assert!(controller.is_editing_text(), "the second editor is open");
    match controller.preview() {
        Some(Preview::Text { at, content, .. }) => {
            assert_eq!(at, point(400.0, 500.0));
            assert!(content.is_empty());
        }
        other => panic!("expected a text preview, got {other:?}"),
    }
}

#[test]
fn reaching_for_another_tool_keeps_the_text() {
    // Losing a typed label because you picked up the pen would be a harsh way
    // to enforce tidiness.
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "keep me");

    let effects = controller.act(Action::SelectTool(Tool::Pen));

    assert_eq!(
        committed_text(&effects).map(|(text, _)| text),
        Some("keep me".to_owned())
    );
    assert!(!controller.is_editing_text());
}

#[test]
fn the_text_size_follows_the_width_setting() {
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "a");
    let (_, small) = committed_text(&controller.act(Action::CommitText)).expect("a commit");

    controller.act(Action::AdjustWidth(3));
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 400.0),
    });
    type_word(&mut controller, "a");
    let (_, large) = committed_text(&controller.act(Action::CommitText)).expect("a commit");

    assert!(large > small, "{large} should be bigger than {small}");
}

#[test]
fn only_the_text_tool_opens_an_editor() {
    for tool in [Tool::Pen, Tool::Rectangle, Tool::Eraser, Tool::Select] {
        let mut controller = typing();
        controller.act(Action::SelectTool(tool));

        controller.handle(PlatformEvent::PointerDown {
            at: point(200.0, 300.0),
        });

        assert!(!controller.is_editing_text(), "{tool:?} opened an editor");
    }
}

// --- The cursor ------------------------------------------------------------

#[test]
fn the_text_tool_shows_an_i_beam() {
    // Without it there is no way to tell where a click will place the caret:
    // the canvas is transparent and has nothing to aim at.
    let controller = typing();

    assert_eq!(controller.cursor(), ink_app::Cursor::Text);
}

#[test]
fn drawing_tools_show_a_crosshair_and_select_shows_an_arrow() {
    let mut controller = typing();

    for tool in [Tool::Pen, Tool::Highlighter, Tool::Rectangle, Tool::Eraser] {
        controller.act(Action::SelectTool(tool));
        assert_eq!(controller.cursor(), ink_app::Cursor::Crosshair, "{tool:?}");
    }

    controller.act(Action::SelectTool(Tool::Select));
    assert_eq!(controller.cursor(), ink_app::Cursor::Default);
}

#[test]
fn the_chrome_wins_over_the_canvas() {
    // An I-beam over the toolbar would promise typing where there is none.
    let controller = typing();
    let button = controller.toolbar().buttons()[0].bounds;
    let over_button = point(
        (button.min.x + button.max.x) / 2.0,
        (button.min.y + button.max.y) / 2.0,
    );

    assert_eq!(controller.cursor_at(over_button), ink_app::Cursor::Default);
    assert_eq!(
        controller.cursor_at(point(600.0, 600.0)),
        ink_app::Cursor::Text
    );
}

#[test]
fn the_grip_shows_a_move_cursor() {
    let controller = typing();
    let grip = controller.toolbar().grip();

    let cursor = controller.cursor_at(point(
        (grip.min.x + grip.max.x) / 2.0,
        (grip.min.y + grip.max.y) / 2.0,
    ));

    assert_eq!(cursor, ink_app::Cursor::Move);
}

#[test]
fn the_resize_corner_shows_a_resize_cursor() {
    let mut controller = typing();
    controller.set_surface_size(ink_core::LogicalSize::new(800.0, 600.0).unwrap());
    let corner = controller.resize_corner().expect("a corner");

    let cursor = controller.cursor_at(point(
        (corner.min.x + corner.max.x) / 2.0,
        (corner.min.y + corner.max.y) / 2.0,
    ));

    assert_eq!(cursor, ink_app::Cursor::ResizeBottomRight);
}

#[test]
fn pass_through_shows_an_ordinary_arrow() {
    // The canvas is not ours there, so a crosshair would claim otherwise.
    let mut controller = typing();
    let transition = match controller.act(Action::ToggleDraw).first() {
        Some(Effect::ApplyMode { transition, .. }) => *transition,
        other => panic!("expected a mode request, got {other:?}"),
    };
    controller.handle(PlatformEvent::ModeApplied { transition });

    assert_eq!(controller.cursor(), ink_app::Cursor::Default);
}

#[test]
fn text_starts_large_enough_to_read_across_a_room() {
    // An annotation over a shared screen is read by an audience, not by the
    // person whose face is against the monitor.
    let mut controller = typing();
    controller.handle(PlatformEvent::PointerDown {
        at: point(200.0, 300.0),
    });
    type_word(&mut controller, "a");

    let (_, size) = committed_text(&controller.act(Action::CommitText)).expect("a commit");

    assert!(
        size >= 24.0,
        "the default text size is {size}, which is small"
    );
}

#[test]
fn the_text_size_is_its_own_setting() {
    // It shared one with the shape tools at first, so resizing a label
    // silently changed how thick the next rectangle came out.
    let mut controller = typing();
    controller.act(Action::SelectTool(Tool::Rectangle));
    let shape_width = controller.style().width;

    controller.act(Action::SelectTool(Tool::Text));
    controller.act(Action::AdjustWidth(2));

    controller.act(Action::SelectTool(Tool::Rectangle));
    assert_eq!(
        controller.style().width,
        shape_width,
        "the rectangle got thicker"
    );
}
