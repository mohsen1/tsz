# fix(checker): include spread-merged props in JSX excess-attribute TS2322 source-type display

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-roadmap-1777442586`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

In `<AnotherComponent {...props} Property1/>` where `Property1` is excess on `AnotherComponentProps`, the JSX excess-property TS2322 emit path at `crates/tsz-checker/src/checkers/jsx/props/resolution.rs:734` hardcoded the source object string to *only* the offending attribute (`{ Property1: true; }`), ignoring everything already pushed into `provided_attrs` by prior spreads.

tsc renders the merged shape: `{ Property1: true; property1: string; property2: number; }` — offender first, then spread-contributed props.

Fix: format `source_display` from the offending attr followed by all other entries already in `provided_attrs`. Matches tsc's source-display order.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` (~30 LOC: replace hardcoded source string with merged formatting in the JSX excess-prop branch).
- `crates/tsz-checker/src/tests/jsx_excess_attr_with_spread_display_tests.rs` (new, ~50 LOC).
- `crates/tsz-checker/src/lib.rs` (1-line module mount).

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2,961 pass (incl. 1 new).
- `cargo nextest run -p tsz-checker jsx` — all 304 JSX tests pass.
- `./scripts/conformance/conformance.sh run --filter "tsxSpreadAttributesResolution14" --verbose` — **flips FAIL → PASS**.
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (full): **12,235 → 12,236, +1 improvement, 0 regressions**.
