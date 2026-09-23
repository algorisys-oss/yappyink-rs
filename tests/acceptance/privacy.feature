# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk privacy contract

  @AC-FR-014 @FR-014 @specified_not_implemented
  Scenario: [AC-FR-014] Minimal privileges
    Given the app runs as a normal user with capture permission denied and without raw-device access
    When I activate basic drawing and pass-through
    Then basic annotation remains usable without privilege escalation or a capture requirement

  @AC-FR-016 @FR-016 @specified_not_implemented
  Scenario: [AC-FR-016] Privacy defaults
    Given default settings and annotations that contain a distinctive sensitive string
    When I draw, hide, inspect application diagnostics, and observe network and filesystem activity
    Then there is no content upload, raw text logging, screenshot retention, or default vector autosave
