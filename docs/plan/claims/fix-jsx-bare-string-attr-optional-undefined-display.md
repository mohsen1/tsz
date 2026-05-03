# fix(checker): preserve `| undefined` in JSX TS2322 target for bare string attrs to optional anonymous props

- **Date**: 2026-05-03
- **Branch**: `fix/jsx-bare-string-attr-optional-undefined-display`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance fingerprint parity

## Intent

tsc's TS2322 message for a bare string-literal JSX attribute initializer
written to an OPTIONAL prop on an ANONYMOUS inline IntrinsicElements type
displays the target as `T | undefined` (e.g. `boolean | undefined`), while
tsz currently strips `| undefined` and shows just `T`. Fixes the fingerprint
mismatch on `tsxAttributeResolution6.tsx` (line 11) without disturbing the
other JSX paths (shorthand attrs, JSX-expression initializers, named-source
props) where tsc strips `| undefined`.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` (~50 LOC change)
- `crates/tsz-checker/src/checkers/jsx/props/validation.rs` (+50 LOC: two helpers)
- `crates/tsz-checker/src/checkers/jsx/tests.rs` (+90 LOC: 3 new unit tests)

## Verification

- `cargo nextest run -p tsz-checker --lib` — 3185 / 3185 passing
- `cargo nextest run -p tsz-solver --lib` — 5590 / 5590 passing
- `./scripts/conformance/conformance.sh run --filter "tsxAttribute"` — 21 / 21 passing
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` — net +1 (tsxAttributeResolution6 now passes)
