# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk overlay contract

  @AC-FR-001 @FR-001 @specified_not_implemented
  Scenario: [AC-FR-001] Live desktop overlay
    Given a supported native environment and an underlying test app with a continuously moving marker
    When I display ink above the app without requesting capture
    Then the marker continues moving beneath the ink and switching ordinary apps does not turn the overlay into a static screenshot

  @AC-FR-002 @FR-002 @specified_not_implemented
  Scenario: [AC-FR-002] Draw mode
    Given Draw mode with a completely blank transparent canvas above a click counter
    When I start and finish a stroke in a never-painted area
    Then one stroke is created and the underlying counter receives no click

  @AC-FR-003 @FR-003 @specified_not_implemented
  Scenario: [AC-FR-003] True pass-through
    Given visible committed ink above the counter, scroll area, and text field of the underlying fixture
    When I enter PassThrough, click the counter once, scroll, click the text field, and type
    Then the counter increments exactly once, scrolling and typing work, ink stays visible, and the app does not synthesize or forward those input events

  @AC-FR-004 @FR-004 @specified_not_implemented
  Scenario: [AC-FR-004] Hide without clearing
    Given committed ink on the selected output
    When I hide the annotation surfaces and then show the annotations
    Then no hidden surface intercepts input and the same committed objects reappear without being cleared

  @AC-FR-005 @FR-005 @specified_not_implemented
  Scenario: [AC-FR-005] Activation and escape
    Given another app has focus and an approved global action or configured desktop command is available
    When I activate Draw and then invoke EmergencyHide, and repeat with the tray unavailable
    Then activation and withdrawal work without a tray and shortcut conflicts or denial are visible rather than silently ignored

  @AC-FR-006 @FR-006 @specified_not_implemented
  Scenario: [AC-FR-006] Toolbar isolation
    Given the floating toolbar is visible in Draw mode
    When I click a tool and drag the toolbar to a different position
    Then the toolbar updates or moves without creating a canvas stroke and remains within the selected output

  @AC-FR-011 @FR-011 @specified_not_implemented
  Scenario: [AC-FR-011] Output-local document
    Given two output documents with distinct annotations
    When I switch the selected output twice
    Then each document retains its own objects and neither depends on an assumed universal global desktop origin

  @AC-FR-012 @FR-012 @specified_not_implemented
  Scenario: [AC-FR-012] Coordinate correctness
    Given the recorded scale and rotation matrix includes 1.0, 1.25, 1.5, and 2.0 scale fixtures
    When I draw and render at known output-local logical coordinates
    Then input and rendered positions agree within the documented tolerance and the same logical width scales correctly

  @AC-FR-013 @FR-013 @specified_not_implemented
  Scenario: [AC-FR-013] Capabilities and honest fallback
    Given a backend lacks layer-shell, visible pass-through, or an approved shortcut route
    When I open diagnostics and attempt the unavailable operation
    Then its capability is reported honestly and a limited compatibility mode is not labeled as full support

  @AC-FR-018 @FR-018 @specified_not_implemented
  Scenario: [AC-FR-018] Gesture and transition safety
    Given a transient stroke and a mouse button that remains held during a mode change
    When I request a handoff or EmergencyHide and later enter Draw again
    Then the canceled preview is not committed and a new stroke requires a fresh pointer press without leaking a partial drag into the underlying app

  @AC-FR-019 @FR-019 @specified_not_implemented
  Scenario: [AC-FR-019] Fault recovery
    Given a pending mode transition with a recoverable surface failure and an older delayed callback
    When the failure and stale callback are delivered
    Then interactive surfaces withdraw, committed ink remains in the model, and the stale event cannot report a false effective mode

  @AC-NFR-005 @NFR-005 @specified_not_implemented
  Scenario: [AC-NFR-005] Failure visibility
    Given a backend reports unsupported, permission denied, shortcut conflict, disconnect, surface loss, or invalid input
    When the application handles each error class
    Then the result remains distinguishable, user-actionable, and never becomes fake native success
