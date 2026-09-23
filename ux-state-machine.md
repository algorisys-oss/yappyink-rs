# Interaction, input, and state contract

## Stable states

| Mode | Ink | Canvas pointer/wheel | Keyboard policy | Toolbar |
|---|---|---|---|---|
| Hidden | Not displayed | No interception | No interception | Hidden |
| Draw | Displayed | Receives annotation gestures across selected output | Explicit overlay focus for tool actions; text focus is a separate V1 substate | Visible |
| PassThrough | Displayed | Goes to underlying applications | Overlay releases keyboard ownership; only registered global actions remain | Hidden by default |

The independent toolbar may later be user-pinned in PassThrough. That feature must specify that the toolbar rectangle is interactive while the canvas is not. The MVP keeps it hidden to make behavior unambiguous.

Additional internal states: Transitioning and Faulted. These are not alternative successful drawing modes. Desired and effective state can differ during a native transition.

## Actions

**ToggleDraw:** Hidden or PassThrough -> Draw. Draw -> PassThrough. If the backend cannot provide PassThrough, do not silently substitute Hidden and claim success. Disable the full-mode action or require the user to explicitly opt into a labeled Draw/Hide compatibility mode.

**ToggleVisibility:** any visible stable mode -> Hidden. Hidden -> PassThrough when supported. It never unexpectedly takes drawing input merely to show old annotations.

**EnterDraw:** request Draw directly. This is useful for a toolbar/launcher command.

**EmergencyHide:** from any reachable mode, cancel transient edits and request immediate withdrawal of overlay and toolbar input/visibility. This action must not wait for file saving, confirmation dialogs, or a capture permission prompt.

**Escape in Draw:** cancel an active transient gesture/text composition first. When there is no transient operation, request PassThrough. Escape is not globally captured in PassThrough and must continue to belong to the underlying app.

**Clear:** clear committed objects on the active output through one undoable command. Hidden/PassThrough do not globally hijack the ordinary Delete or Ctrl/Cmd+Z shortcuts.

## Pointer sequence safety

When switching modes during an owned gesture, cancel the uncommitted preview. Do not transfer a half-finished drag to the underlying app. Where the backend needs to wait until buttons are released for a normal Draw -> PassThrough handoff, expose the transition state and stop drawing; EmergencyHide still withdraws immediately. On returning to Draw, ignore preexisting held buttons until release, then accept only a new pointer-down.

This behavior must be tested on real backends because mouse hit-test transparency alone does not establish focus or capture semantics.

## Focus handoff

Mouse transparency and keyboard focus are separate properties. The backend must release any owned keyboard grab/focus policy and prevent the ink surface from reactivating itself during PassThrough. Restore the previous application's focus only through permitted native mechanisms. Do not promise to forcibly focus arbitrary applications on Wayland.

Acceptance requires that clicking the underlying app then typing works normally without disappearing ink. A no-extra-click keyboard restoration path is desirable and should be reported as a separately measured capability where OS policy limits it. Do not synthesize the missing click or keystrokes.

## Startup and recovery

Startup is Hidden, with no blocking full-screen surface. Resolve the active output from saved/user selection; on platforms that cannot observe a global cursor while Hidden, show a monitor choice instead of guessing a cursor location.

A missing tray is not fatal. Supply a launcher/settings window and a local CLI recovery route. A shortcut conflict is visible and does not leave an overlay active without a reliable exit. If a handler is never run because the whole process has hung, an in-process shortcut cannot guarantee rescue; document native OS process termination. Test normal recoverable failure cleanup separately from an unresponsive process.

## Suggested toolbar

```text
Move | Pen | Highlighter | Line | Arrow | Rectangle | Ellipse | Eraser
Color | Width | Opacity | Undo | Redo | Clear | Interact | Hide | Settings
```

Group secondary actions in a menu on small displays. Use tooltips and selected-tool text, not only color. A toolbar drag must not draw on the canvas. Keep controls inside the selected output's usable bounds after resizing/hotplug.

Do not hard-code globally conflict-prone defaults without registration feedback. Use platform-appropriate Ctrl versus Command for local editing shortcuts. Final default global chords are chosen during the spike against each desktop's reserved bindings.
