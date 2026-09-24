//! Input methods, through `zwp_text_input_v3`.
//!
//! Composing in Devanagari, CJK and many other scripts does not produce one
//! character per key press. The keystrokes belong to an input method, which
//! composes them and hands back finished text, so reading key events directly
//! is not a partial solution for those scripts but no solution at all.
//!
//! What this file owns is the conversation with the engine. The rules about
//! what a composition means live in `ink-app`, which is why they are testable.
//!
//! The protocol is double buffered like the rest of Wayland: the engine sends
//! `preedit_string`, `commit_string` and `delete_surrounding_text` in any
//! order, and none of them take effect until `done`. Applying them as they
//! arrive would show half-applied states, so they are accumulated and flushed
//! on `done`.

use ink_app::PlatformEvent;
use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::{Connection, Proxy, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ContentHint, ContentPurpose, Event as TextInputEvent, ZwpTextInputV3,
};

use crate::overlay::Overlay;

/// What the engine has said since the last `done`.
#[derive(Default)]
pub struct Pending {
    pub preedit: Option<String>,
    pub commit: Option<String>,
    pub delete: Option<(u32, u32)>,
}

/// User data for the text-input object.
///
/// SCTK's `delegate_dispatch2!` implements `Dispatch` for every interface whose
/// user data implements `Dispatch2`, so a protocol it does not know about is
/// added by giving its object a data type like this one rather than by writing
/// another `Dispatch` impl, which would collide with the blanket one.
pub struct TextInputData;

impl Dispatch2<ZwpTextInputV3, Overlay> for TextInputData {
    fn event(
        &self,
        state: &mut Overlay,
        _proxy: &ZwpTextInputV3,
        event: TextInputEvent,
        _conn: &Connection,
        _qh: &QueueHandle<Overlay>,
    ) {
        match event {
            // Every event from the engine is logged, including the empty ones.
            // Silence here and silence from a disengaged engine look identical
            // otherwise, and telling them apart is the whole question when
            // text comes out in the wrong script. See learning.md §8 and §13.
            TextInputEvent::PreeditString { text, .. } => {
                let text = text.unwrap_or_default();
                eprintln!("[ime] preedit {text:?}");
                state.ime_pending.preedit = Some(text);
            }
            TextInputEvent::CommitString { text } => {
                let text = text.unwrap_or_default();
                eprintln!("[ime] commit {text:?}");
                state.ime_pending.commit = Some(text);
            }
            TextInputEvent::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                eprintln!("[ime] delete {before_length} before, {after_length} after");
                state.ime_pending.delete = Some((before_length, after_length));
            }
            TextInputEvent::Done { .. } => {
                let pending = std::mem::take(&mut state.ime_pending);
                // Order matters and is the protocol's: delete first, then the
                // commit, then whatever composition remains. Applying them in
                // any other order rewrites the wrong characters.
                if let Some((before, after)) = pending.delete {
                    let effects = state
                        .controller
                        .handle(PlatformEvent::DeleteSurrounding { before, after });
                    state.apply(effects);
                }
                if let Some(text) = pending.commit {
                    let effects = state.controller.handle(PlatformEvent::CommitPreedit(text));
                    state.apply(effects);
                }
                // Always sent, even when empty: an engine clears a composition
                // by sending no preedit at all, and the editor has to notice
                // that rather than keep showing the last one.
                let effects = state
                    .controller
                    .handle(PlatformEvent::Preedit(pending.preedit.unwrap_or_default()));
                state.apply(effects);
                state.needs_redraw = true;
            }
            // Text-input focus follows keyboard focus, and `enable` is only
            // meaningful while focused: a client that enables before being
            // told it has focus is talking to nobody, which is why the engine
            // never engaged and every key arrived as a plain keystroke.
            TextInputEvent::Enter { .. } => {
                eprintln!("[ime] focused");
                state.ime_focused = true;
                // Focus can arrive after the editor is already open, so the
                // engine is told immediately rather than waiting for the next
                // change.
                state.sync_input_method();
            }
            TextInputEvent::Leave { .. } => {
                eprintln!("[ime] unfocused");
                state.ime_focused = false;
                // The compositor has already disabled the object on its side;
                // saying so here keeps the two in step, so the next focus
                // enables again rather than assuming it is still on.
                state.ime_enabled = false;
            }
            _ => {}
        }
    }
}

/// User data for the manager. It sends no events; this exists so the blanket
/// dispatch has something to call.
pub struct TextInputManagerData;

impl Dispatch2<ZwpTextInputManagerV3, Overlay> for TextInputManagerData {
    fn event(
        &self,
        _state: &mut Overlay,
        _proxy: &ZwpTextInputManagerV3,
        _event: <ZwpTextInputManagerV3 as Proxy>::Event,
        _conn: &Connection,
        _qh: &QueueHandle<Overlay>,
    ) {
    }
}

/// Tells the engine an editor is open and where its caret is.
///
/// The rectangle is what positions the candidate window. Without it the list of
/// suggestions appears wherever the compositor guesses, which over a
/// full-screen overlay is nowhere useful.
pub fn enable(input: &ZwpTextInputV3, caret: (i32, i32, i32, i32)) {
    input.enable();
    input.set_content_type(ContentHint::None, ContentPurpose::Normal);
    let (x, y, width, height) = caret;
    input.set_cursor_rectangle(x, y, width, height);
    input.commit();
}

/// Tells the engine the editor has closed.
///
/// FR-023 wants text focus released on leaving the editor, and with an input
/// method involved that means telling the engine too: one that still believes
/// a text field is focused will keep composing into nothing.
pub fn disable(input: &ZwpTextInputV3) {
    input.disable();
    input.commit();
}

/// Updates the caret rectangle while typing, so the candidate window follows.
pub fn move_caret(input: &ZwpTextInputV3, caret: (i32, i32, i32, i32)) {
    let (x, y, width, height) = caret;
    input.set_cursor_rectangle(x, y, width, height);
    input.commit();
}
