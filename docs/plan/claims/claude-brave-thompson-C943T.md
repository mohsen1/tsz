# fix(checker): suppress JSX per-attribute type check after any-typed spread

- **Date**: 2026-05-01
- **Branch**: `claude/brave-thompson-C943T`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance (JSX spread attribute resolution)

## Intent

When a JSX element contains an `any`/`error`/`unknown`-typed spread
attribute, tsc treats the merged JSX-attributes object as
`any`-compatible and suppresses per-attribute TS2322 mismatches against
the props type. tsz did not, producing an extra TS2322 in the
`tsxSpreadAttributesResolution12` conformance test (and any element of
the form `<Comp {...anyobj} attr={...} />` where `attr` would otherwise
mismatch).

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` — pre-scan
  attribute list for `any`/`error`/`unknown` spreads; gate the existing
  per-attribute skip path on the new flag (~30 LOC).
- `crates/tsz-checker/src/checkers/jsx/tests.rs` — three unit tests
  covering the explicit-attr-after-any-spread, shorthand-attr-after-
  any-spread, and the no-any-spread sanity case (~70 LOC).

## Verification

- `cargo nextest run --package tsz-checker --lib` — 3069 passed
- `cargo fmt --all --check` — clean
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  — clean
- `./scripts/conformance/conformance.sh run --filter
  tsxSpreadAttributesResolution12 --verbose` — fingerprint-only failure
  reduced from 3 extras to 2 extras (the spurious `'3' to '2'` at
  `x={3}` after `{...anyobj}` is gone). Test remains in the
  fingerprint-only category pending follow-up work on the merged
  spread-source display and per-attribute anchoring.
- `./scripts/conformance/conformance.sh run --max 200` — 200/200.
- Full conformance: net -3 from pre-existing flaky tests (5 PASSes are
  silently dropped on every run; each individually returns PASS via
  `--filter`). None of the flaky tests touch JSX.
