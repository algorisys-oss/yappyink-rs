# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk v1 contract

  @AC-FR-021 @FR-021 @specified_not_implemented
  Scenario: [AC-FR-021] Simultaneous multiple outputs
    Given two active output surfaces at different scales
    When I drag toward an output edge and continue moving across the display boundary
    Then the stroke does not jump coordinate systems and a fresh press is required to begin a stroke on the other output

  @AC-FR-022 @FR-022 @specified_not_implemented
  Scenario: [AC-FR-022] Hotplug and session lifecycle
    Given committed ink and an active gesture on an output
    When I unplug the output, reconnect it, change scale, and exercise lock or suspend lifecycle fixtures
    Then the gesture cancels safely, no invisible input blocker remains, and committed content is preserved with an explicit remapping choice when needed

  @AC-FR-023 @FR-023 @specified_not_implemented
  Scenario: [AC-FR-023] Text and editing
    Given an active text editor with an IME composition and a selected annotation object
    When I commit and cancel text compositions, move the object, and undo the move
    Then Unicode and preedit behave correctly, keyboard ownership is explicit, and undo restores the object without leaking input to another app

  @AC-FR-024 @FR-024 @specified_not_implemented
  Scenario: [AC-FR-024] Annotation-only export
    Given a vector scene and capture permission denied
    When I export ink-only PNG and SVG
    Then the exports contain the expected objects and transparency without desktop pixels, app controls, or a screen-capture prompt

  @AC-FR-029 @FR-029 @specified_not_implemented
  Scenario: [AC-FR-029] Live zoom
    Given a video is playing under the overlay on a platform that offers zoom
    When I zoom in, draw a stroke while zoomed, and zoom out again
    Then the magnified view keeps updating and the stroke lands under the pointer
    And where the compositor magnifies, the stroke stays on the content it marks and no desktop pixels reach the application
    And where yappyink captures instead, permission was asked once, refusing it left the overlay working, and no frame outlives the screen
    And quitting while zoomed leaves the screen, and any setting of the user's, as it was
