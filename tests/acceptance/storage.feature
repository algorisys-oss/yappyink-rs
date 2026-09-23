# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk storage contract

  @AC-FR-015 @FR-015 @specified_not_implemented
  Scenario: [AC-FR-015] Explicit local document files
    Given a known vector document and an explicit user-selected destination
    When I save the document and load it into an empty session
    Then objects, styles, and output-local coordinates roundtrip without containing pixels of the underlying desktop

  @AC-FR-017 @FR-017 @specified_not_implemented
  Scenario: [AC-FR-017] Safe file and settings handling
    Given a valid saved document and a different active document
    When I inject a write failure and separately load malformed, oversized, nonfinite, or newer-schema data
    Then the previous valid file and current document remain intact with a typed actionable error

  @AC-FR-020 @FR-020 @specified_not_implemented
  Scenario: [AC-FR-020] Settings and diagnostics
    Given valid saved tool settings and a conflicted or corrupted shortcut preference
    When I restart Hidden and open doctor or the settings recovery path
    Then valid settings load, invalid values are explained, a recovery control remains available, and diagnostics omit user content
