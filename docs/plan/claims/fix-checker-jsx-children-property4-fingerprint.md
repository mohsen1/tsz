# fix(checker): align JSX children property diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-jsx-children-property4-fingerprint`
- **PR**: #2812
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the fingerprint-only `checkJsxChildrenProperty4` conformance slice. `tsz`
currently emits the same diagnostic codes as `tsc` (`TS2322`, `TS2551`) but
differs in diagnostic position or message details for JSX `children` property
checking.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/children.rs`
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs`

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker`
- `cargo test -p tsz-checker --test jsx_component_attribute_tests jsx_react_multiple_render_prop_children_contextual_type_uses_declared_callback -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "checkJsxChildrenProperty4" --verbose` (1/1 passed, 100%, 0 fingerprint-only)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed, 100%, 0 fingerprint-only)

`cargo nextest` availability is unknown in this environment; targeted
`cargo test` was used for local verification.

## Notes

The React class-component fallback for multiple render-prop children correctly
checked each child against the React multiple-children target, but it also used
that target as the contextual type. That made arrow parameters fall back to
`any`, so the diagnostic displayed `(user: any) => Element`. `tsc` contextually
types those arrows from the declared callable `children` prop while still
reporting the mismatch against the React child target.

The fix separates the contextual child type from the assignability target for
that fallback and anchors function-expression JSX children on the inner arrow
expression, matching the `user =>` column in the conformance fixture.
