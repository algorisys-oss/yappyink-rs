# Acceptance test status

The `.feature` files are specification examples, not executable automated tests. No step definitions or runner are provided. All scenarios are `specified_not_implemented` and all native environments are untested for this application.

Implement headless unit/property tests for domain behavior, adapter contract tests with a fake platform, renderer checks with documented tolerances, and real native desktop acceptance runs. Map actual test names/evidence locations back into `traceability.json` as implementation proceeds.

The manual underlay fixture is `fixtures/underlay.html`. Open it in a browser, then run the future overlay above it. It provides a click counter, editable text, scrolling, and motion to reveal accidental screenshot-freeze behavior. Opening this page alone does not test the Rust application.

See `quality-gates.md` for the distinction between compilation, mocks, and verified desktop behavior.
