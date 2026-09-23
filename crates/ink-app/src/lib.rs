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

use ink_core::LogicalPoint;
use ink_platform::PlatformError;

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
    /// A gesture finished. The caller turns these points into an object with
    /// the current tool and style, which is why no style appears here.
    CommitStroke { points: Vec<LogicalPoint> },
    /// A gesture was discarded. Carries the sample count for diagnostics; the
    /// points themselves are gone deliberately, so nothing can resurrect them.
    GestureCancelled { points: usize },
    /// A transition failed. Always preceded by [`Effect::WithdrawImmediately`].
    Faulted { error: PlatformError },
}

/// What the pointer is doing.
#[derive(Clone, Debug)]
enum Gesture {
    /// Nothing in flight. A press starts drawing.
    Idle,
    /// Collecting samples for a stroke that has not been committed.
    Drawing { points: Vec<LogicalPoint> },
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

    /// The samples of the gesture in flight, if any. Not yet a document edit.
    pub fn gesture_points(&self) -> Option<&[LogicalPoint]> {
        match &self.gesture {
            Gesture::Drawing { points } => Some(points),
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
        self.gesture = Gesture::Drawing { points: vec![at] };
        Vec::new()
    }

    fn pointer_moved(&mut self, at: LogicalPoint) -> Vec<Effect> {
        if let Gesture::Drawing { points } = &mut self.gesture {
            points.push(at);
        }
        Vec::new()
    }

    fn pointer_up(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.button_held = false;

        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Drawing { mut points } => {
                points.push(at);
                // One gesture is one object, however many samples it took
                // (FR-007). A press and release without movement is a dot.
                vec![Effect::CommitStroke { points }]
            }
            // The release that ends an ignored drag. Nothing is committed, and
            // the next press starts clean.
            Gesture::IgnoringHeldButton | Gesture::Idle => Vec::new(),
        }
    }

    /// Discards any gesture in flight, reporting how much was thrown away.
    fn cancel_gesture(&mut self) -> Option<Effect> {
        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Drawing { points } => Some(Effect::GestureCancelled {
                points: points.len(),
            }),
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
