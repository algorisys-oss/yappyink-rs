# Contract scenarios only. No runner or step definitions are implemented.
# Unsupported environments are blocked/not applicable as documented, never counted as passing.
Feature: ScreenInk drawing contract

  @AC-FR-007 @FR-007 @specified_not_implemented
  Scenario: [AC-FR-007] Pen and highlighter
    Given an empty document and a pen or translucent highlighter with a known logical width
    When I make a dot, a stroke with self-overlap, and a canceled gesture
    Then there are exactly two committed objects, width and opacity match the style, and internal highlighter mesh overlaps do not create dark seams

  @AC-FR-008 @FR-008 @specified_not_implemented
  Scenario: [AC-FR-008] Basic shapes
    Given each basic shape tool and an empty history
    When I drag one valid shape and cancel a second preview
    Then one object and one history transaction exist and the canceled preview is absent

  @AC-FR-009 @FR-009 @specified_not_implemented
  Scenario: [AC-FR-009] Object eraser
    Given objects A, B, and C where A and C intersect a fast eraser sweep
    When I erase across A and C and undo the eraser gesture once
    Then A and C are restored in original order with original styles and B remains unchanged

  @AC-FR-010 @FR-010 @specified_not_implemented
  Scenario: [AC-FR-010] Undo, redo, and clear
    Given a known sequence of add, delete, and clear transactions
    When I undo and redo the transactions, then undo one and commit a different edit
    Then document states match their expected snapshots and redo is cleared only by the new committed edit
