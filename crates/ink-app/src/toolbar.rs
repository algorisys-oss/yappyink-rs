//! The compact floating toolbar (FR-006).
//!
//! Layout and hit testing only: no drawing, no platform. What matters about a
//! toolbar is not how it looks but who owns the pointer when it is pressed, and
//! that is a rule worth testing.
//!
//! FR-006 forbids "an imaginary clickable button on an entirely click-through
//! surface". The toolbar here is part of the overlay surface and is only
//! offered while Draw mode owns pointer input, so a button that looks
//! interactive is interactive. In PassThrough the surface takes no pointer
//! input at all, so the toolbar is hidden rather than left on screen looking
//! usable (`ux-state-machine.md` keeps it hidden in the MVP for exactly this
//! reason).
//!
//! `architecture.md` proposes a separate `ink-ui` crate for this. It lives here
//! instead, because the contract being implemented is pointer arbitration
//! between canvas and controls, which is the controller's job; a separate crate
//! would add a hop without adding a boundary. The same document says to avoid a
//! crate per noun and to split when a boundary contains real code. When the
//! toolbar grows a settings panel, that is the time.

use ink_core::{LogicalPoint, LogicalRect};

use crate::{Action, Mode, Tool};

/// Button edge length, in logical units.
const BUTTON: f64 = 30.0;
/// Space between buttons.
const GAP: f64 = 4.0;
/// Space between the toolbar's edge and its buttons.
const PADDING: f64 = 6.0;
/// Distance from the surface's top-left corner.
const ORIGIN: f64 = 10.0;
/// Width of the grip used to drag the whole overlay.
const GRIP: f64 = 16.0;

/// What a button does, and how it is drawn.
///
/// The icon is named for the concept rather than the picture, so the drawing
/// code can change without this meaning anything different.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Pen,
    Highlighter,
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Eraser,
    Undo,
    Redo,
    Clear,
    PassThrough,
    Hide,
}

/// One control.
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub icon: Icon,
    pub action: Action,
    pub bounds: LogicalRect,
}

impl Button {
    /// Whether this button represents the tool currently in use.
    ///
    /// Drawn with a mark as well as a colour change: NFR-006 requires a
    /// selected state that is not carried by colour alone.
    pub fn is_selected(&self, tool: Tool) -> bool {
        matches!(self.action, Action::SelectTool(selected) if selected == tool)
    }
}

/// The toolbar's buttons and where they are.
#[derive(Clone, Debug)]
pub struct Toolbar {
    buttons: Vec<Button>,
    bounds: LogicalRect,
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl Toolbar {
    pub fn new() -> Self {
        // Tools first, then history, then the two ways out. Grouped by what
        // the user is thinking about rather than by how often each is pressed.
        let entries = [
            (Icon::Pen, Action::SelectTool(Tool::Pen)),
            (Icon::Highlighter, Action::SelectTool(Tool::Highlighter)),
            (Icon::Line, Action::SelectTool(Tool::Line)),
            (Icon::Arrow, Action::SelectTool(Tool::Arrow)),
            (Icon::Rectangle, Action::SelectTool(Tool::Rectangle)),
            (Icon::Ellipse, Action::SelectTool(Tool::Ellipse)),
            (Icon::Eraser, Action::SelectTool(Tool::Eraser)),
            (Icon::Undo, Action::Undo),
            (Icon::Redo, Action::Redo),
            (Icon::Clear, Action::Clear),
            (Icon::PassThrough, Action::ToggleDraw),
            (Icon::Hide, Action::ToggleVisibility),
        ];

        let buttons: Vec<Button> = entries
            .iter()
            .enumerate()
            .map(|(index, (icon, action))| {
                let x = ORIGIN + PADDING + GRIP + GAP + index as f64 * (BUTTON + GAP);
                let y = ORIGIN + PADDING;
                Button {
                    icon: *icon,
                    action: *action,
                    bounds: LogicalRect {
                        min: LogicalPoint { x, y },
                        max: LogicalPoint {
                            x: x + BUTTON,
                            y: y + BUTTON,
                        },
                    },
                }
            })
            .collect();

        let width = GRIP + GAP + entries.len() as f64 * (BUTTON + GAP) - GAP + PADDING * 2.0;
        let bounds = LogicalRect {
            min: LogicalPoint {
                x: ORIGIN,
                y: ORIGIN,
            },
            max: LogicalPoint {
                x: ORIGIN + width,
                y: ORIGIN + BUTTON + PADDING * 2.0,
            },
        };

        Self { buttons, bounds }
    }

    /// Whether the toolbar is offered in this mode.
    ///
    /// Draw only. In PassThrough the surface accepts no pointer input, so
    /// showing controls there would be showing something that cannot be
    /// clicked, which is the specific thing FR-006 forbids.
    pub fn is_visible(mode: Mode) -> bool {
        mode == Mode::Draw
    }

    pub fn buttons(&self) -> &[Button] {
        &self.buttons
    }

    /// The grip that drags the whole overlay.
    ///
    /// xdg-shell gives a client no way to place its own window, so the only
    /// way to move an overlay is to ask the compositor to run the drag
    /// (E003 finding 4). A grip is that request's handle.
    pub fn grip(&self) -> LogicalRect {
        LogicalRect {
            min: LogicalPoint {
                x: self.bounds.min.x + PADDING,
                y: self.bounds.min.y + PADDING,
            },
            max: LogicalPoint {
                x: self.bounds.min.x + PADDING + GRIP,
                y: self.bounds.max.y - PADDING,
            },
        }
    }

    /// Whether a point is on the grip.
    pub fn is_grip(&self, at: LogicalPoint) -> bool {
        contains(self.grip(), at)
    }

    /// The whole toolbar's rectangle, including its padding.
    pub fn bounds(&self) -> LogicalRect {
        self.bounds
    }

    /// The button at a point, if any.
    pub fn hit(&self, at: LogicalPoint) -> Option<&Button> {
        self.buttons
            .iter()
            .find(|button| contains(button.bounds, at))
    }

    /// Whether a point is anywhere on the toolbar, including its padding.
    ///
    /// Used to decide pointer ownership: a press on the frame between buttons
    /// belongs to the toolbar, not to the canvas, or the gap would be a hole
    /// you could accidentally draw through.
    pub fn contains(&self, at: LogicalPoint) -> bool {
        contains(self.bounds, at)
    }
}

fn contains(rect: LogicalRect, at: LogicalPoint) -> bool {
    at.x >= rect.min.x && at.x <= rect.max.x && at.y >= rect.min.y && at.y <= rect.max.y
}
