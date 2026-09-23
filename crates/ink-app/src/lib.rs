//! Controller and mode coordinator.
//!
//! This crate holds the rules that decide *what should happen*. It touches no
//! window, surface, or input device: it takes user actions and platform events
//! in, and returns [`Effect`]s for an adapter to carry out. That keeps NFR-004
//! satisfied, and it is why the whole state machine can be tested headlessly
//! and deterministically.
//!
//! Two ideas carry most of the weight:
//!
//! **Desired is not effective.** Asking for PassThrough does not make us in
//! PassThrough. The platform confirms, and only then is the mode effective
//! (FR-019). Claiming otherwise would mean telling the user their clicks reach
//! the application underneath while the overlay is still swallowing them.
//!
//! **Every transition is numbered.** A confirmation or failure carrying an old
//! [`TransitionId`] is ignored, so a callback that arrives late cannot
//! overwrite a state the user has since moved on from.

#![forbid(unsafe_code)]

pub mod toolbar;

pub use toolbar::{Button, Icon, Toolbar};

use ink_core::{
    DocumentError, LogicalPoint, LogicalRect, LogicalSize, Opacity, Rgb, Shape, StrokeKind, Style,
    Width, limits,
};
use ink_platform::PlatformError;

/// The gesture in flight, ready to be drawn as a preview.
///
/// Borrowed rather than owned: a stroke's samples can run to six figures, and
/// cloning them for every frame of the preview would be the kind of per-sample
/// copy `architecture.md` warns against.
#[derive(Clone, Debug)]
pub enum Preview<'a> {
    Stroke {
        points: &'a [LogicalPoint],
        kind: StrokeKind,
        style: Style,
    },
    /// Already-built geometry. Cheap, because a shape is two points.
    Shape { shape: Shape, style: Style },
    /// The path an eraser has swept so far, so the user can see what it is
    /// about to take.
    Erase {
        path: &'a [LogicalPoint],
        radius: f64,
    },
}

/// A drawing tool (FR-007, FR-008).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Opaque freehand ink.
    Pen,
    /// Wide translucent ink. Its opacity applies to the completed stroke as a
    /// whole, which the renderer is responsible for; the document only records
    /// the value.
    Highlighter,
    Line,
    /// A line with a head at the end the drag finished on.
    Arrow,
    Rectangle,
    Ellipse,
    /// Removes whole objects its sweep touches (FR-009).
    Eraser,
}

impl Tool {
    /// Whether this tool follows the pointer, rather than being defined by
    /// where a drag started and ended.
    pub fn is_freehand(self) -> bool {
        matches!(self, Self::Pen | Self::Highlighter | Self::Eraser)
    }

    /// How a stroke drawn with this tool is stored, for the freehand tools.
    pub fn stroke_kind(self) -> Option<StrokeKind> {
        match self {
            Self::Pen => Some(StrokeKind::Pen),
            Self::Highlighter => Some(StrokeKind::Highlighter),
            _ => None,
        }
    }

    /// Builds the geometry for a completed drag between two points.
    ///
    /// Returns the same [`DocumentError`] the document would: a drag that went
    /// nowhere is refused here rather than becoming an object nobody can see
    /// (FR-008).
    fn shape_from_drag(self, from: LogicalPoint, to: LogicalPoint) -> Result<Shape, DocumentError> {
        match self {
            Self::Line => Shape::line(from, to),
            Self::Arrow => Shape::arrow(from, to),
            Self::Rectangle => Shape::rectangle(from, to),
            Self::Ellipse => Shape::ellipse(from, to),
            // A freehand tool is not defined by its endpoints.
            Self::Pen | Self::Highlighter | Self::Eraser => Err(DocumentError::EmptyStroke),
        }
    }
}

/// The default palette.
///
/// A starting set, not a designed one. Real colour configuration is T019, and
/// a picker belongs on the toolbar (T013). These are chosen to stay legible
/// over both dark and light windows.
const PALETTE: [Rgb; 6] = [
    Rgb::new(255, 0, 255),
    Rgb::new(255, 64, 64),
    Rgb::new(255, 208, 0),
    Rgb::new(64, 224, 96),
    Rgb::new(64, 176, 255),
    Rgb::new(255, 255, 255),
];

/// Width steps, in logical units.
const WIDTHS: [f64; 7] = [1.0, 2.0, 4.0, 6.0, 10.0, 16.0, 24.0];
/// Opacity steps. Nothing reaches 0.0: a fully invisible tool is a trap.
const OPACITIES: [f64; 5] = [0.15, 0.3, 0.5, 0.75, 1.0];

/// One tool's settings.
#[derive(Clone, Copy, Debug)]
struct ToolState {
    colour: usize,
    width: usize,
    opacity: usize,
}

impl ToolState {
    fn style(self) -> Style {
        Style::new(
            PALETTE[self.colour % PALETTE.len()],
            Width::new(WIDTHS[self.width.min(WIDTHS.len() - 1)])
                .expect("table entries are positive"),
            Opacity::new(OPACITIES[self.opacity.min(OPACITIES.len() - 1)])
                .expect("table entries are in range"),
        )
    }

    /// Moves `steps` through a table, stopping at either end rather than
    /// wrapping: a user holding the thicker key should not suddenly get the
    /// thinnest line.
    fn step(current: usize, steps: i32, len: usize) -> usize {
        let target = current as i64 + i64::from(steps);
        target.clamp(0, len as i64 - 1) as usize
    }
}

/// A stable, user-visible mode.
///
/// `Transitioning` and `Faulted` from `ux-state-machine.md` are not variants
/// here: they are not modes a user can be in, they are conditions. Ask with
/// [`Controller::is_transitioning`] and [`Controller::is_faulted`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No ink, no interception. Committed annotations are retained (FR-004).
    Hidden,
    /// The canvas receives annotation gestures across the output (FR-002).
    Draw,
    /// Ink stays visible; pointer, wheel, and keyboard belong to the
    /// applications underneath (FR-003).
    PassThrough,
}

/// Identifies one attempt to change mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransitionId(u64);

impl TransitionId {
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Something the user asked for, however it was triggered: a shortcut, a
/// toolbar button, or a CLI command all arrive here identically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Hidden or PassThrough to Draw; Draw to PassThrough.
    ToggleDraw,
    /// Any visible mode to Hidden; Hidden to PassThrough.
    ToggleVisibility,
    /// Request Draw directly.
    EnterDraw,
    /// Cancel a gesture if one is in flight, otherwise request PassThrough.
    Escape,
    /// Withdraw everything now. Never waits for confirmation, a save, or a
    /// permission prompt.
    EmergencyHide,
    /// Choose the pen or the highlighter.
    SelectTool(Tool),
    /// Set the current tool's colour. Off-palette colours are kept as given.
    SetColor(Rgb),
    /// Move to the next colour in the palette, wrapping.
    CycleColor,
    /// Step the current tool's width up or down the table.
    AdjustWidth(i32),
    /// Step the current tool's opacity up or down the table.
    AdjustOpacity(i32),
    /// Undo the last committed edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Remove every object on the active output, as one undoable edit.
    Clear,
}

/// Something that happened outside the controller.
#[derive(Clone, Debug)]
pub enum PlatformEvent {
    /// The platform finished applying the mode for this transition.
    ModeApplied {
        transition: TransitionId,
    },
    /// The platform could not apply the mode for this transition.
    ModeFailed {
        transition: TransitionId,
        error: PlatformError,
    },
    PointerDown {
        at: LogicalPoint,
    },
    PointerMoved {
        at: LogicalPoint,
    },
    PointerUp {
        at: LogicalPoint,
    },
    /// The platform took the pointer away mid-gesture: a grab was broken, the
    /// surface lost focus, the device disappeared.
    PointerCancelled,
    /// The output the overlay lives on went away (FR-019, FR-022).
    OutputLost,
}

/// Work for an adapter to carry out.
///
/// Returned in the order they must happen. Where ordering matters it is
/// load-bearing, not cosmetic: a withdrawal is emitted before the fault that
/// caused it, so a failure cannot leave an invisible surface intercepting
/// input while an error is reported.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Apply this mode natively, then report back with this transition id.
    ApplyMode {
        mode: Mode,
        transition: TransitionId,
    },
    /// Withdraw every interactive surface immediately, without awaiting
    /// confirmation.
    WithdrawImmediately,
    /// A gesture finished and should become one object. Carries the style
    /// captured when it began, so a setting changed mid-gesture cannot
    /// rewrite what the user drew.
    CommitObject { shape: Shape, style: Style },
    /// A gesture was cancelled. Carries the sample count for diagnostics; the
    /// points themselves are gone deliberately, so nothing can resurrect them.
    GestureCancelled { points: usize },
    /// A gesture finished but produced nothing usable, such as a drag that
    /// went nowhere. Distinct from a cancellation: the user completed this
    /// one, so they may need telling why nothing appeared (FR-008).
    GestureDiscarded { reason: DocumentError },
    /// An eraser sweep finished. The session resolves which objects it
    /// touched, because that needs the document, and one gesture becomes one
    /// undoable transaction however many objects it removed (FR-009).
    EraseAlong {
        path: Vec<LogicalPoint>,
        radius: f64,
    },
    /// Undo the last committed edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Remove everything, as one undoable edit.
    Clear,
    /// Ask the compositor to move the overlay, following the pointer.
    ///
    /// The compositor runs the drag; a Wayland client cannot place its own
    /// window (E003 finding 4). Issued on press, because that is when the
    /// serial the compositor requires is available and when a drag begins.
    BeginWindowDrag,
    /// Ask the compositor to resize the overlay from its bottom-right corner.
    ///
    /// Worth having because this backend cannot go fullscreen without losing
    /// transparency (E002 finding 1), so enlarging the window by hand is the
    /// only way to annotate more of the screen.
    BeginWindowResize,
    /// A transition failed. Always preceded by [`Effect::WithdrawImmediately`].
    Faulted { error: PlatformError },
}

/// Edge length of the resize grab area in the bottom-right corner, in logical
/// units.
const RESIZE_CORNER: f64 = 22.0;

/// How far a sample must be from the previous one to be worth keeping, in
/// logical units.
///
/// A pointer reporting at 1000 Hz over a slow drag produces samples far below
/// a pixel apart. Storing them costs memory and rendering time and changes
/// nothing a person can see (NFR-003).
const MIN_SAMPLE_DISTANCE: f64 = 0.5;

fn rect_contains(rect: LogicalRect, at: LogicalPoint) -> bool {
    at.x >= rect.min.x && at.x <= rect.max.x && at.y >= rect.min.y && at.y <= rect.max.y
}

/// What the pointer is doing.
#[derive(Clone, Debug)]
enum Gesture {
    /// Nothing in flight. A press starts drawing.
    Idle,
    /// Collecting samples for a stroke that has not been committed. The tool
    /// and style are fixed at the press.
    Drawing {
        points: Vec<LogicalPoint>,
        kind: StrokeKind,
        style: Style,
    },
    /// An eraser sweep. The path is kept so that the segments between samples
    /// can be tested, not only the samples themselves.
    Sweeping {
        path: Vec<LogicalPoint>,
        radius: f64,
    },
    /// A shape being dragged out. Only the two endpoints matter, so a long
    /// drag costs nothing to keep.
    Dragging {
        tool: Tool,
        from: LogicalPoint,
        to: LogicalPoint,
        style: Style,
    },
    /// The pointer went down on the toolbar. Nothing is drawn, and the
    /// button acts on release if the pointer is still on it, which is how a
    /// button is meant to behave and also why a drag starting on the toolbar
    /// cannot leave ink (FR-006).
    OnToolbar { icon: crate::toolbar::Icon },
    /// A button was already down when Draw became effective. Everything from
    /// it is ignored until it is released, so a drag begun in another mode
    /// cannot turn into ink (`ux-state-machine.md`, "Pointer sequence safety").
    IgnoringHeldButton,
}

/// A mode change that has been requested but not confirmed.
#[derive(Clone, Copy, Debug)]
struct Pending {
    transition: TransitionId,
    target: Mode,
}

/// The mode and gesture state machine.
#[derive(Debug)]
pub struct Controller {
    desired: Mode,
    effective: Mode,
    pending: Option<Pending>,
    next_transition: u64,
    gesture: Gesture,
    /// Last known state of the drawing button. The platform may stop telling
    /// us once we are in PassThrough, so this is a best-effort memory used to
    /// decide whether a resumed Draw should ignore a held button.
    button_held: bool,
    faulted: bool,
    toolbar: Toolbar,
    /// The surface's size in logical units, once the compositor has said.
    /// Needed to know where the resize corner is.
    surface: Option<LogicalSize>,
    tool: Tool,
    pen: ToolState,
    highlighter: ToolState,
    /// Shared by the four shape tools. They are one pen held differently, so
    /// switching between a rectangle and an arrow should not change the
    /// colour under the user.
    shape: ToolState,
    /// Only the width is meaningful: an eraser has no colour of its own.
    eraser: ToolState,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller {
    /// A controller in Hidden, which is where startup begins (FR-004).
    pub fn new() -> Self {
        Self {
            desired: Mode::Hidden,
            effective: Mode::Hidden,
            pending: None,
            next_transition: 1,
            gesture: Gesture::Idle,
            button_held: false,
            faulted: false,
            toolbar: Toolbar::new(),
            surface: None,
            tool: Tool::Pen,
            pen: ToolState {
                colour: 0,
                width: 2,
                opacity: OPACITIES.len() - 1,
            },
            // Wider and translucent, or it would be a pen by another name.
            highlighter: ToolState {
                colour: 2,
                width: 5,
                opacity: 1,
            },
            shape: ToolState {
                colour: 0,
                width: 2,
                opacity: OPACITIES.len() - 1,
            },
            eraser: ToolState {
                colour: 0,
                width: 4,
                opacity: OPACITIES.len() - 1,
            },
        }
    }

    /// Tells the controller how big the surface is.
    ///
    /// The compositor decides the size, so this arrives with every configure.
    pub fn set_surface_size(&mut self, size: LogicalSize) {
        self.surface = Some(size);
    }

    /// The corner that starts a resize, if the surface size is known.
    pub fn resize_corner(&self) -> Option<LogicalRect> {
        let size = self.surface?;
        let grab = RESIZE_CORNER
            .min(size.width() / 2.0)
            .min(size.height() / 2.0);
        Some(LogicalRect {
            min: LogicalPoint {
                x: size.width() - grab,
                y: size.height() - grab,
            },
            max: LogicalPoint {
                x: size.width(),
                y: size.height(),
            },
        })
    }

    /// The toolbar, for drawing it and for tests.
    pub fn toolbar(&self) -> &Toolbar {
        &self.toolbar
    }

    /// Whether the toolbar should be drawn right now.
    pub fn toolbar_visible(&self) -> bool {
        Toolbar::is_visible(self.effective)
    }

    /// The selected tool.
    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// The selected tool's current style.
    pub fn style(&self) -> Style {
        self.tool_state().style()
    }

    fn tool_state(&self) -> &ToolState {
        match self.tool {
            Tool::Pen => &self.pen,
            Tool::Highlighter => &self.highlighter,
            Tool::Eraser => &self.eraser,
            _ => &self.shape,
        }
    }

    fn tool_state_mut(&mut self) -> &mut ToolState {
        match self.tool {
            Tool::Pen => &mut self.pen,
            Tool::Highlighter => &mut self.highlighter,
            Tool::Eraser => &mut self.eraser,
            _ => &mut self.shape,
        }
    }

    /// The mode the platform has confirmed.
    pub fn mode(&self) -> Mode {
        self.effective
    }

    /// The mode the user has asked for, which may not be in effect yet.
    pub fn desired_mode(&self) -> Mode {
        self.desired
    }

    pub fn is_transitioning(&self) -> bool {
        self.pending.is_some()
    }

    pub fn is_faulted(&self) -> bool {
        self.faulted
    }

    /// The samples of the freehand gesture in flight, if any. Not yet a
    /// document edit.
    pub fn gesture_points(&self) -> Option<&[LogicalPoint]> {
        match &self.gesture {
            Gesture::Drawing { points, .. } => Some(points),
            _ => None,
        }
    }

    /// Whether any gesture is in flight, freehand or shape.
    pub fn is_gesturing(&self) -> bool {
        matches!(
            self.gesture,
            Gesture::Drawing { .. } | Gesture::Dragging { .. }
        )
    }

    /// The gesture in flight, ready to draw.
    ///
    /// The style is the one captured at the press, so a preview cannot
    /// disagree with what will be committed. A shape whose drag has not gone
    /// anywhere yet previews as nothing, which matches what committing it
    /// would do.
    pub fn preview(&self) -> Option<Preview<'_>> {
        match &self.gesture {
            Gesture::Drawing {
                points,
                kind,
                style,
            } => Some(Preview::Stroke {
                points,
                kind: *kind,
                style: *style,
            }),
            Gesture::Sweeping { path, radius } => Some(Preview::Erase {
                path,
                radius: *radius,
            }),
            Gesture::Dragging {
                tool,
                from,
                to,
                style,
            } => tool
                .shape_from_drag(*from, *to)
                .ok()
                .map(|shape| Preview::Shape {
                    shape,
                    style: *style,
                }),
            _ => None,
        }
    }

    /// Handles a user action.
    pub fn act(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::EmergencyHide => self.emergency_hide(),
            Action::Escape => self.escape(),
            Action::EnterDraw => self.request(Mode::Draw),
            Action::ToggleDraw => match self.desired {
                Mode::Draw => self.request(Mode::PassThrough),
                Mode::Hidden | Mode::PassThrough => self.request(Mode::Draw),
            },
            Action::ToggleVisibility => match self.desired {
                Mode::Hidden => self.request(Mode::PassThrough),
                Mode::Draw | Mode::PassThrough => self.request(Mode::Hidden),
            },
            // Tool changes never touch a gesture in flight. The stroke keeps
            // the tool and style it was started with, so a setting changed
            // mid-drag cannot rewrite what the user drew.
            Action::SelectTool(tool) => {
                self.tool = tool;
                Vec::new()
            }
            Action::SetColor(colour) => {
                // Only palette colours can be selected today, because the
                // style is derived from an index. Storing an arbitrary colour
                // needs a settings model, which is T019; until then an
                // off-palette request is ignored rather than silently
                // substituted with something else.
                if let Some(index) = PALETTE.iter().position(|entry| *entry == colour) {
                    self.tool_state_mut().colour = index;
                }
                Vec::new()
            }
            Action::CycleColor => {
                let next = (self.tool_state().colour + 1) % PALETTE.len();
                self.tool_state_mut().colour = next;
                Vec::new()
            }
            Action::AdjustWidth(steps) => {
                let current = self.tool_state().width;
                self.tool_state_mut().width = ToolState::step(current, steps, WIDTHS.len());
                Vec::new()
            }
            Action::AdjustOpacity(steps) => {
                let current = self.tool_state().opacity;
                self.tool_state_mut().opacity = ToolState::step(current, steps, OPACITIES.len());
                Vec::new()
            }
            // History actions cancel a gesture in flight first. Undoing while
            // half-way through a stroke would otherwise leave a preview on
            // screen belonging to a document state that no longer exists.
            Action::Undo => self.with_gesture_cancelled(Effect::Undo),
            Action::Redo => self.with_gesture_cancelled(Effect::Redo),
            Action::Clear => self.with_gesture_cancelled(Effect::Clear),
        }
    }

    /// Handles a platform event.
    pub fn handle(&mut self, event: PlatformEvent) -> Vec<Effect> {
        match event {
            PlatformEvent::ModeApplied { transition } => self.mode_applied(transition),
            PlatformEvent::ModeFailed { transition, error } => self.mode_failed(transition, error),
            PlatformEvent::PointerDown { at } => self.pointer_down(at),
            PlatformEvent::PointerMoved { at } => self.pointer_moved(at),
            PlatformEvent::PointerUp { at } => self.pointer_up(at),
            PlatformEvent::PointerCancelled => {
                self.button_held = false;
                self.cancel_gesture().into_iter().collect()
            }
            PlatformEvent::OutputLost => self.output_lost(),
        }
    }

    // --- mode changes ------------------------------------------------------

    /// Emits an effect, cancelling any gesture in flight first.
    fn with_gesture_cancelled(&mut self, effect: Effect) -> Vec<Effect> {
        let mut effects: Vec<Effect> = self.cancel_gesture().into_iter().collect();
        effects.push(effect);
        effects
    }

    /// Requests a mode, cancelling any gesture in flight.
    ///
    /// A request for the mode already desired is a no-op and produces nothing,
    /// so a user mashing a shortcut cannot queue up redundant native work.
    fn request(&mut self, mode: Mode) -> Vec<Effect> {
        if self.desired == mode && !self.faulted {
            return Vec::new();
        }

        // FR-018: a gesture in flight never survives a mode change. It is not
        // committed, and it is not handed to the application underneath.
        let mut effects: Vec<Effect> = self.cancel_gesture().into_iter().collect();

        let transition = self.new_transition();
        self.desired = mode;
        self.pending = Some(Pending {
            transition,
            target: mode,
        });
        effects.push(Effect::ApplyMode { mode, transition });
        effects
    }

    fn escape(&mut self) -> Vec<Effect> {
        if self.effective != Mode::Draw {
            // Escape belongs to the application underneath. Consuming it here
            // would be indistinguishable from stealing a keystroke.
            return Vec::new();
        }
        if let Some(cancelled) = self.cancel_gesture() {
            return vec![cancelled];
        }
        self.request(Mode::PassThrough)
    }

    fn emergency_hide(&mut self) -> Vec<Effect> {
        let mut effects: Vec<Effect> = self.cancel_gesture().into_iter().collect();

        // Deliberately asymmetric with every other transition. Showing or
        // intercepting needs confirmation before it is called effective;
        // withdrawing does not, because the failure directions are not
        // symmetric either. Claiming to be hidden while still visible is
        // recoverable and visible to the user; making them wait for a
        // confirmation that never arrives is not.
        self.desired = Mode::Hidden;
        self.effective = Mode::Hidden;
        // Abandoning the pending transition makes any confirmation still in
        // flight stale, so a late success cannot un-hide us.
        self.pending = None;
        self.gesture = Gesture::Idle;

        effects.push(Effect::WithdrawImmediately);
        effects
    }

    fn mode_applied(&mut self, transition: TransitionId) -> Vec<Effect> {
        let Some(pending) = self.pending else {
            return Vec::new();
        };
        if pending.transition != transition {
            return Vec::new();
        }

        self.pending = None;
        self.effective = pending.target;
        self.faulted = false;

        if pending.target == Mode::Draw && self.button_held {
            // The user was already holding the button when Draw came back.
            // Wait for a fresh press before drawing anything.
            self.gesture = Gesture::IgnoringHeldButton;
        }
        Vec::new()
    }

    fn mode_failed(&mut self, transition: TransitionId, error: PlatformError) -> Vec<Effect> {
        let Some(pending) = self.pending else {
            return Vec::new();
        };
        if pending.transition != transition {
            return Vec::new();
        }

        self.pending = None;
        self.desired = Mode::Hidden;
        self.effective = Mode::Hidden;
        self.faulted = true;
        self.gesture = Gesture::Idle;

        // Order matters: withdraw first, report second. A fault that reported
        // before withdrawing could leave a surface intercepting input while a
        // dialog asks the user what to do about it.
        vec![Effect::WithdrawImmediately, Effect::Faulted { error }]
    }

    fn output_lost(&mut self) -> Vec<Effect> {
        let mut effects: Vec<Effect> = self.cancel_gesture().into_iter().collect();
        self.desired = Mode::Hidden;
        self.effective = Mode::Hidden;
        self.pending = None;
        self.gesture = Gesture::Idle;
        // Committed objects are untouched: the document outlives the surface.
        effects.push(Effect::WithdrawImmediately);
        effects
    }

    // --- gestures ----------------------------------------------------------

    fn pointer_down(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.button_held = true;

        if self.effective != Mode::Draw || self.is_transitioning() {
            // Not ours to draw with. In PassThrough we should not be receiving
            // this at all, and mid-transition we refuse to start something the
            // next state would have to throw away.
            return Vec::new();
        }
        if matches!(self.gesture, Gesture::IgnoringHeldButton) {
            // Cannot happen from a real device, which must release before it
            // can press again. Treated as a fresh press rather than trusted.
            return Vec::new();
        }
        // FR-006: a press on the toolbar belongs to the toolbar. This is
        // checked before anything else, so no tool can ever draw over its own
        // controls, and the gaps between buttons are not holes to draw
        // through.
        if self.toolbar_visible() && self.toolbar.contains(at) {
            if self.toolbar.is_grip(at) {
                // The compositor takes the pointer for the duration of the
                // drag, so nothing here must be left armed.
                self.gesture = Gesture::IgnoringHeldButton;
                return vec![Effect::BeginWindowDrag];
            }
            let icon = self.toolbar.hit(at).map(|button| button.icon);
            self.gesture = match icon {
                Some(icon) => Gesture::OnToolbar { icon },
                // The frame between buttons: swallow the press without arming
                // anything.
                None => Gesture::IgnoringHeldButton,
            };
            return Vec::new();
        }

        // The resize corner is offered only where the surface takes input,
        // for the same reason the toolbar is.
        if self.toolbar_visible()
            && self
                .resize_corner()
                .is_some_and(|corner| rect_contains(corner, at))
        {
            self.gesture = Gesture::IgnoringHeldButton;
            return vec![Effect::BeginWindowResize];
        }

        let style = self.style();
        self.gesture = match self.tool {
            // The eraser's width is the diameter of what it takes, so the
            // radius is half of it, matching what the user sees.
            Tool::Eraser => Gesture::Sweeping {
                path: vec![at],
                radius: style.width.get() / 2.0,
            },
            tool => match tool.stroke_kind() {
                Some(kind) => Gesture::Drawing {
                    points: vec![at],
                    kind,
                    style,
                },
                // A shape is defined by where the drag started and where it
                // ends, so the samples in between are not kept at all.
                None => Gesture::Dragging {
                    tool,
                    from: at,
                    to: at,
                    style,
                },
            },
        };
        Vec::new()
    }

    fn pointer_moved(&mut self, at: LogicalPoint) -> Vec<Effect> {
        if let Gesture::Dragging { to, .. } = &mut self.gesture {
            *to = at;
            return Vec::new();
        }
        if let Gesture::Sweeping { path, .. } = &mut self.gesture {
            // Bounded and thinned like a stroke: the sweep is geometry too.
            if path.len() < limits::MAX_STROKE_POINTS
                && path.last().is_none_or(|last| {
                    (at.x - last.x).abs() >= MIN_SAMPLE_DISTANCE
                        || (at.y - last.y).abs() >= MIN_SAMPLE_DISTANCE
                })
            {
                path.push(at);
            }
            return Vec::new();
        }
        let Gesture::Drawing { points, .. } = &mut self.gesture else {
            return Vec::new();
        };

        // NFR-003: bound what a single gesture can store. A high-rate pointer
        // over a long drag would otherwise accumulate samples nobody can see.
        if points.len() >= limits::MAX_STROKE_POINTS {
            return Vec::new();
        }
        if let Some(last) = points.last()
            && (at.x - last.x).abs() < MIN_SAMPLE_DISTANCE
            && (at.y - last.y).abs() < MIN_SAMPLE_DISTANCE
        {
            // Too close to the previous sample to change the shape.
            return Vec::new();
        }
        points.push(at);
        Vec::new()
    }

    fn pointer_up(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.button_held = false;

        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Drawing {
                mut points,
                kind,
                style,
            } => {
                // The release point completes the shape, so it is kept even
                // when the thinning rule would have dropped it. A press and
                // release in one place stays a single-sample dot, which FR-007
                // calls a valid gesture.
                let moved = points.last().is_none_or(|last| {
                    (at.x - last.x).abs() >= MIN_SAMPLE_DISTANCE
                        || (at.y - last.y).abs() >= MIN_SAMPLE_DISTANCE
                });
                if moved && points.len() < limits::MAX_STROKE_POINTS {
                    points.push(at);
                }
                // One gesture is one object, however many samples it took.
                match Shape::stroke(kind, points) {
                    Ok(shape) => vec![Effect::CommitObject { shape, style }],
                    Err(reason) => vec![Effect::GestureDiscarded { reason }],
                }
            }
            Gesture::Dragging {
                tool, from, style, ..
            } => {
                // The release point defines the shape, not the last sample
                // seen during the drag.
                match tool.shape_from_drag(from, at) {
                    Ok(shape) => vec![Effect::CommitObject { shape, style }],
                    // A drag that went nowhere. FR-008: it must not become an
                    // object the user cannot see, select, or erase, and the
                    // reason travels with it rather than vanishing.
                    Err(reason) => vec![Effect::GestureDiscarded { reason }],
                }
            }
            Gesture::Sweeping { mut path, radius } => {
                if path.last().is_none_or(|last| {
                    (at.x - last.x).abs() >= MIN_SAMPLE_DISTANCE
                        || (at.y - last.y).abs() >= MIN_SAMPLE_DISTANCE
                }) {
                    path.push(at);
                }
                vec![Effect::EraseAlong { path, radius }]
            }
            Gesture::OnToolbar { icon } => {
                // Only if the pointer is still on the button it went down on,
                // which is how a user cancels a press by sliding off it.
                let still_there = self
                    .toolbar
                    .hit(at)
                    .filter(|button| button.icon == icon)
                    .map(|button| button.action);
                match still_there {
                    Some(action) => self.act(action),
                    None => Vec::new(),
                }
            }
            // The release that ends an ignored drag. Nothing is committed, and
            // the next press starts clean.
            Gesture::IgnoringHeldButton | Gesture::Idle => Vec::new(),
        }
    }

    /// Discards any gesture in flight, reporting how much was thrown away.
    fn cancel_gesture(&mut self) -> Option<Effect> {
        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Drawing { points, .. } => Some(Effect::GestureCancelled {
                points: points.len(),
            }),
            Gesture::Dragging { .. } => Some(Effect::GestureCancelled { points: 2 }),
            Gesture::Sweeping { path, .. } => Some(Effect::GestureCancelled { points: path.len() }),
            // Nothing was being drawn, so there is nothing to report.
            Gesture::OnToolbar { .. } => None,
            Gesture::IgnoringHeldButton | Gesture::Idle => None,
        }
    }

    fn new_transition(&mut self) -> TransitionId {
        let id = TransitionId(self.next_transition);
        self.next_transition = self
            .next_transition
            .checked_add(1)
            .expect("transition ids exhausted");
        id
    }
}
