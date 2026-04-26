# fix(checker): expand Application-backed Intersection for branded-primitive TS2739 messages

- **Date**: 2026-04-26
- **Branch**: `claude/exciting-keller-raRPb`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance fingerprint parity

## Intent

tsc always shows the structural Intersection form (e.g. `Number & { __brand: T }`) for branded
primitive types in TS2739 assignability messages, rather than the Application alias form
(e.g. `Brand<T>`). This PR fixes the type formatter to:
1. Skip Application display-alias for Intersections that contain at least one primitive member.
2. Capitalize primitive members in intersections (`number` → `Number`, `string` → `String`,
   `boolean` → `Boolean`), matching tsc's apparent-type display for branded primitives.

Root cause: the display_alias mechanism redirected branded Intersection types back to their
Application origin. The check was missing for the Intersection-with-Application-alias case
(only the Object-with-Intersection-alias case was handled).

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/mod.rs` — new flags and skip logic
- `crates/tsz-solver/src/diagnostics/format/compound.rs` — primitive capitalization in intersection members
- `crates/tsz-solver/src/diagnostics/format/tests.rs` — 3 new unit tests
- `crates/tsz-checker/src/error_reporter/core_formatting.rs` — detection + dispatch for the new path

## Verification

- `cargo check --package tsz-checker` — clean
- `cargo check --package tsz-solver` — clean
- `cargo test --package tsz-solver -- capitalize_primitive skip_application_alias` — 3 pass
- `./scripts/conformance/conformance.sh run --filter intersectionAsWeakTypeSource --verbose` — PASS
- `./scripts/conformance/conformance.sh run --max 200` — 200/200, no regressions
- Full suite: see PR CI
