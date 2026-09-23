# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk quality contract

  @AC-NFR-001 @NFR-001 @specified_not_implemented
  Scenario: [AC-NFR-001] Drawing responsiveness
    Given the recorded release-build benchmark fixture and reference hardware
    When I measure normalized-input-dispatch-to-render-submit times while drawing
    Then the report includes p50 p95 and p99, compares p95 with the proposed 16.7 ms target, and does not call submission timing input-to-photon latency

  @AC-NFR-002 @NFR-002 @specified_not_implemented
  Scenario: [AC-NFR-002] Idle efficiency
    Given Hidden mode and then unchanged visible ink with no animations
    When I observe render scheduling and sample CPU use for 60 seconds on the reference machine
    Then there is no continuous redraw loop and the report compares Hidden CPU with the proposed one-core-normalized target

  @AC-NFR-003 @NFR-003 @specified_not_implemented
  Scenario: [AC-NFR-003] Responsiveness and memory bounds
    Given input, history, document size, and worker queues reach configured boundaries
    When I continue drawing and inject slow I/O or portal responses
    Then the UI remains responsive, memory is bounded, ordering-critical input is preserved, and limits are reported without silent corruption

  @AC-NFR-004 @NFR-004 @specified_not_implemented
  Scenario: [AC-NFR-004] Testable core
    Given the domain/reducer package in an environment without a display or GPU
    When I run its deterministic unit and property tests
    Then they exercise drawing and history without linking an OS window or graphics backend

  @AC-NFR-007 @NFR-007 @specified_not_implemented
  Scenario: [AC-NFR-007] Traceable development
    Given an implementation change is proposed for review
    When I inspect its task, requirements, scenarios, test output, and native evidence
    Then every behavior change is traceable and no task is marked verified from a scenario file or mock test alone
