//! Global chords through Carbon's `RegisterEventHotKey` (FR-005).
//!
//! Carbon is old, and this corner of it is still the supported way to bind a
//! system-wide chord without asking for accessibility permission; it is what
//! the menu-bar utilities people already run use. The alternative, a
//! `CGEventTap` or a global `NSEvent` monitor, needs the user to grant this
//! program permission to read every key they type, which is a great deal to ask
//! of an annotation overlay. ADR-007 records the choice.
//!
//! There is no Rust binding for this part of Carbon among our dependencies, so
//! the five functions are declared here by hand. Each signature is copied from
//! `CarbonEvents.h`; the constants they take are in [`crate::chords`], where
//! they are tested.
//!
//! **Never pressed.** Nobody on this project has a Mac, and the one launch of
//! the macOS build so far (E009) predates this code.

use std::ffi::c_void;

use ink_app::Action;

use crate::chords::{self, CHORDS};

type OsStatus = i32;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventRef = *mut c_void;
type EventHotKeyRef = *mut c_void;

/// `EventHandlerUPP`. On 64-bit, a universal procedure pointer is just the
/// function pointer.
type EventHandler =
    Option<unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OsStatus>;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
#[derive(Default)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandler,
        type_count: usize,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out_handler: *mut EventHandlerRef,
    ) -> OsStatus;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: EventTargetRef,
        options: u32,
        out_hot_key: *mut EventHotKeyRef,
    ) -> OsStatus;
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        buffer_size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OsStatus;
}

/// `noErr`.
const NO_ERR: OsStatus = 0;
/// `eventNotHandledErr`, which lets the event continue to other handlers.
const EVENT_NOT_HANDLED: OsStatus = -9874;

thread_local! {
    /// Where a fired chord goes. A plain function rather than a closure,
    /// because Carbon calls back through a C function pointer. Thread-local
    /// because Carbon delivers hot key events on the main thread's run loop,
    /// which is the thread that registered them.
    static ON_CHORD: std::cell::Cell<Option<fn(Action)>> = const { std::cell::Cell::new(None) };
}

unsafe extern "C" fn handle(
    _call: EventHandlerCallRef,
    event: EventRef,
    _user_data: *mut c_void,
) -> OsStatus {
    let mut id = EventHotKeyId::default();
    let status = unsafe {
        GetEventParameter(
            event,
            chords::PARAM_DIRECT_OBJECT,
            chords::TYPE_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyId>(),
            std::ptr::null_mut(),
            (&mut id as *mut EventHotKeyId).cast(),
        )
    };
    if status != NO_ERR {
        return EVENT_NOT_HANDLED;
    }
    let Some(action) = chords::action_for(id.signature, id.id) else {
        return EVENT_NOT_HANDLED;
    };
    if let Some(on_chord) = ON_CHORD.with(std::cell::Cell::get) {
        on_chord(action);
    }
    NO_ERR
}

/// Installs the handler and registers every chord, reporting each one.
///
/// A refused chord is reported, not fatal: the overlay still works, it just
/// lacks that way back, and the user is told which.
///
/// Must be called on the main thread, before the application's run loop
/// starts.
pub fn register(on_chord: fn(Action)) {
    ON_CHORD.with(|slot| slot.set(Some(on_chord)));

    let target = unsafe { GetApplicationEventTarget() };
    let pressed = EventTypeSpec {
        event_class: chords::EVENT_CLASS_KEYBOARD,
        event_kind: chords::EVENT_HOT_KEY_PRESSED,
    };
    let mut handler: EventHandlerRef = std::ptr::null_mut();
    let status = unsafe {
        InstallEventHandler(
            target,
            Some(handle),
            1,
            &pressed,
            std::ptr::null_mut(),
            &mut handler,
        )
    };
    if status != NO_ERR {
        eprintln!(
            "[hotkey] the Carbon event handler was refused (status {status}), so no chord \
             works and the only way back from pass-through is this terminal"
        );
        return;
    }

    for chord in CHORDS {
        let mut registered: EventHotKeyRef = std::ptr::null_mut();
        let id = EventHotKeyId {
            signature: chords::SIGNATURE,
            id: chord.id,
        };
        let status = unsafe {
            RegisterEventHotKey(chord.key, chord.modifiers, id, target, 0, &mut registered)
        };
        if status == NO_ERR {
            eprintln!("[hotkey] {} registered", chord.name);
        } else {
            // -9878 is eventHotKeyExistsErr: another application has it.
            eprintln!(
                "[hotkey] {} was refused (status {status}); another application may own it",
                chord.name
            );
        }
    }
    // The handler and the hot keys live until the process exits. Carbon
    // removes both then, and there is no earlier moment at which dropping
    // them would be right.
}
