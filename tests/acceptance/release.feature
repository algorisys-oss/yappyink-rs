# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk release contract

  @AC-FR-025 @FR-025 @specified_not_implemented
  Scenario: [AC-FR-025] Distribution and verification
    Given a proposed supported environment and its packaged release candidate
    When I install it on a clean machine and run the applicable native acceptance matrix
    Then support is published only with recorded passing evidence and unsupported or untested environments are clearly marked

  @AC-NFR-006 @NFR-006 @specified_not_implemented
  Scenario: [AC-NFR-006] Accessible controls
    Given each certified UI backend and its platform accessibility tooling
    When I navigate toolbar and settings using keyboard and screen-reader controls
    Then actions have labels, selection is not color-only, focus is visible, and any unverified accessibility behavior blocks a blanket accessibility claim

  @AC-NFR-008 @NFR-008 @specified_not_implemented
  Scenario: [AC-NFR-008] Dependency and release hygiene
    Given a release candidate with its actual toolchain and dependency lockfile
    When I perform reproducible build, license, unsafe-code, package, and signing checks for its selected targets
    Then the exact versions and results are recorded and no unverified target is presented as supported
