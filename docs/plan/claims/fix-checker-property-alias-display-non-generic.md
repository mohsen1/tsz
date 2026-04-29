# fix(checker): preserve simple type-alias names in TS2339 receiver display

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-roadmap-1777447697`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints — type-display-parity Tier 1 campaign)

## Intent

`property_receiver_display_for_node` (`crates/tsz-checker/src/error_reporter/properties.rs`) bridges the property-error type display from the receiver's source-text annotation when:

1. the annotation contains `<` (a generic instantiation), AND
2. the type is a generic Application or has a `display_alias`.

The first condition rejected non-generic alias annotations like `bar: Bar` where `type Bar = Omit<Foo, "c">`. tsz then fell through to `format_type` which expanded the alias to `Omit<Foo, "c">`, while tsc preserves `Bar` because that's the alias the source code referred to. The reason: tsz's `display_alias` only tracks one level back (to the Application), so without the annotation-bridge there is no path back to the alias name.

Fix: drop the `contains('<')` requirement. Simple-alias annotations now also flow through `format_annotation_like_type`, matching tsc's TS2339 wording.

## Files Touched

- `crates/tsz-checker/src/error_reporter/properties.rs` (~10 LOC: relax annotation-bridge condition).
- `crates/tsz-checker/src/tests/property_alias_display_tests.rs` (new, 3 regression tests).
- `crates/tsz-checker/src/lib.rs` (1-line module mount).

## Verification

- `cargo nextest run -p tsz-checker --lib` (full suite incl. 3 new locks).
- `./scripts/conformance/conformance.sh run --filter "omitTypeTestErrors01"` — flips FAIL → PASS.
- Full conformance run pending; pre-rebase run showed +6 net (with 9 improvements, 3 regressions including pre-existing main-only ones).
