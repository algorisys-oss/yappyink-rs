# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk later contract

  @AC-FR-026 @FR-026 @specified_not_implemented
  Scenario: [AC-FR-026] Capture and freeze
    Given live overlay is usable and optional capture has not been authorized
    When I cancel or deny capture, then authorize freeze in a separate run
    Then live annotation survives cancellation and freeze clearly blocks interaction with the stale underlying image until it is dismissed

  @AC-FR-027 @FR-027 @specified_not_implemented
  Scenario: [AC-FR-027] Screenshot composite export
    Given a known test background, translucent ink, and a visible toolbar
    When I explicitly capture and export a screenshot composite
    Then the exported image contains the ink exactly once, excludes app controls, has correct scale, and captured pixels are not retained automatically

  @AC-FR-028 @FR-028 @specified_not_implemented
  Scenario: [AC-FR-028] Presenter extras
    Given a proposed presenter enhancement with its own approved spec
    When I enable it on an environment with limited input or capture capabilities
    Then the feature respects its declared mode and permissions and does not introduce hidden raw global input monitoring
