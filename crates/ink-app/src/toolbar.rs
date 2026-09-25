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

use crate::{Action, Mode, PALETTE, Tool};

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
/// Edge length of a colour swatch.
const SWATCH: f64 = 22.0;

/// What a button does, and how it is drawn.
///
/// The icon is named for the concept rather than the picture, so the drawing
/// code can change without this meaning anything different.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Opens the compositor's own window menu, where "Always on Top" lives.
    WindowMenu,
    Select,
    Pen,
    Highlighter,
    Line,
    Arrow,
    Rectangle,
    Ellipse,
    Eraser,
    Delete,
    Undo,
    Redo,
    Clear,
    PassThrough,
    Hide,
    /// Shrinks the overlay to just this toolbar, and back.
    Park,
    /// Leaves the application.
    Quit,
    /// Places a caret and types.
    Text,
    /// Shows the row of colour swatches. Drawn in the current colour, so the
    /// button is itself the answer to "what am I drawing with?".
    Color,
    /// Steps the platform's magnifier through its zoom levels and off
    /// (FR-029). Only on the toolbar where the adapter has a magnifier.
    Zoom,
}

/// One control.
#[derive(Clone, Copy, Debug)]
pub struct Button {
    pub icon: Icon,
    pub action: Action,
    pub bounds: LogicalRect,
}

impl Button {
    /// The tooltip, naming the action and its key.
    ///
    /// Uppercase because the built-in label font has no lowercase: it exists
    /// so chrome can have words without a font stack, and a real one arrives
    /// with the text tool (T028).
    pub fn label(&self) -> &'static str {
        match self.icon {
            Icon::WindowMenu => "ALWAYS ON TOP (T)",
            Icon::Select => "SELECT (S)",
            Icon::Pen => "PEN (1)",
            Icon::Highlighter => "HIGHLIGHTER (2)",
            Icon::Line => "LINE (3)",
            Icon::Arrow => "ARROW (4)",
            Icon::Rectangle => "RECTANGLE (5)",
            Icon::Ellipse => "ELLIPSE (6)",
            Icon::Text => "TEXT (9)",
            Icon::Eraser => "ERASER (E)",
            Icon::Color => "COLOUR (C CYCLES)",
            Icon::Delete => "DELETE SELECTED (DEL)",
            Icon::Undo => "UNDO (U)",
            Icon::Redo => "REDO (R)",
            Icon::Clear => "CLEAR ALL (X)",
            Icon::PassThrough => "PASS THROUGH (P)",
            Icon::Park => "SHRINK TO TOOLBAR (G)",
            Icon::Hide => "HIDE (H)",
            Icon::Quit => "SAVE AND QUIT (Q)",
            Icon::Zoom => "ZOOM 2X 3X 4X (Z), OFF (0)",
        }
    }

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
    /// The toolbar without a zoom button, as on a platform with no magnifier.
    pub fn new() -> Self {
        Self::with_zoom(false)
    }

    /// The toolbar, with a zoom button if the platform can zoom.
    ///
    /// Optional rather than always present: FR-029 says a platform without a
    /// mechanism shows no button, because a button that does nothing is the
    /// control FR-006 forbids.
    pub fn with_zoom(zoom: bool) -> Self {
        // Tools first, then history, then the two ways out. Grouped by what
        // the user is thinking about rather than by how often each is pressed.
        let mut entries = vec![
            (Icon::Select, Action::SelectTool(Tool::Select)),
            (Icon::Pen, Action::SelectTool(Tool::Pen)),
            (Icon::Highlighter, Action::SelectTool(Tool::Highlighter)),
            (Icon::Line, Action::SelectTool(Tool::Line)),
            (Icon::Arrow, Action::SelectTool(Tool::Arrow)),
            (Icon::Rectangle, Action::SelectTool(Tool::Rectangle)),
            (Icon::Ellipse, Action::SelectTool(Tool::Ellipse)),
            (Icon::Text, Action::SelectTool(Tool::Text)),
            (Icon::Eraser, Action::SelectTool(Tool::Eraser)),
            (Icon::Color, Action::ToggleColorPicker),
            (Icon::Delete, Action::DeleteSelection),
            (Icon::Undo, Action::Undo),
            (Icon::Redo, Action::Redo),
            (Icon::Clear, Action::Clear),
            (Icon::WindowMenu, Action::ShowWindowMenu),
            (Icon::PassThrough, Action::ToggleDraw),
            (Icon::Park, Action::TogglePark),
            (Icon::Hide, Action::ToggleVisibility),
            (Icon::Quit, Action::Quit),
        ];
        if zoom {
            // With the other view controls, after the window menu.
            let at = entries
                .iter()
                .position(|(icon, _)| *icon == Icon::PassThrough)
                .unwrap_or(entries.len());
            entries.insert(at, (Icon::Zoom, Action::CycleZoom));
        }

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
    /// Draw and PassThrough, not Hidden.
    ///
    /// It used to be Draw only, because PassThrough gave the surface an empty
    /// input region and a control that cannot be clicked is the specific thing
    /// FR-006 forbids. The fix was not to keep hiding it but to make the claim
    /// true: in PassThrough the input region is now the toolbar's own
    /// rectangle rather than nothing, so the buttons really are clickable and
    /// everything else really does pass through.
    ///
    /// `ux-state-machine.md` anticipated this: "The independent toolbar may
    /// later be user-pinned in PassThrough. That feature must specify that the
    /// toolbar rectangle is interactive while the canvas is not."
    pub fn is_visible(mode: Mode) -> bool {
        matches!(mode, Mode::Draw | Mode::PassThrough | Mode::Parked)
    }

    /// Whether the document's ink is drawn in this mode.
    ///
    /// Parked shrinks the surface to the toolbar, so there is nowhere to draw
    /// it. The document is kept, exactly as in Hidden.
    pub fn ink_is_visible(mode: Mode) -> bool {
        matches!(mode, Mode::Draw | Mode::PassThrough)
    }

    /// Whether the canvas takes pointer input in this mode.
    ///
    /// The other half of the same rule. In PassThrough the toolbar is live and
    /// the canvas is not, so a press outside the toolbar is not ours at all.
    pub fn canvas_is_interactive(mode: Mode) -> bool {
        mode == Mode::Draw
    }

    pub fn buttons(&self) -> &[Button] {
        &self.buttons
    }

    /// The swatch row's rectangle, directly below the toolbar.
    ///
    /// Computed whether or not the picker is open, because the controller
    /// needs the geometry to decide what the input region should cover.
    pub fn swatch_row(&self) -> LogicalRect {
        let left = self
            .buttons
            .iter()
            .find(|button| button.icon == Icon::Color)
            .map_or(self.bounds.min.x, |button| button.bounds.min.x);
        let width = PALETTE.len() as f64 * (SWATCH + GAP) - GAP + PADDING * 2.0;
        let top = self.bounds.max.y + GAP;
        LogicalRect {
            min: LogicalPoint { x: left, y: top },
            max: LogicalPoint {
                x: left + width,
                y: top + SWATCH + PADDING * 2.0,
            },
        }
    }

    /// Each swatch, with its palette position and colour.
    pub fn swatches(&self) -> Vec<(usize, crate::Rgb, LogicalRect)> {
        let row = self.swatch_row();
        PALETTE
            .iter()
            .enumerate()
            .map(|(index, colour)| {
                let x = row.min.x + PADDING + index as f64 * (SWATCH + GAP);
                let y = row.min.y + PADDING;
                (
                    index,
                    *colour,
                    LogicalRect {
                        min: LogicalPoint { x, y },
                        max: LogicalPoint {
                            x: x + SWATCH,
                            y: y + SWATCH,
                        },
                    },
                )
            })
            .collect()
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
