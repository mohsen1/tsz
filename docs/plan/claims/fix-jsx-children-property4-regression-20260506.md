# fix(checker): realign JSX children property fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/jsx-children-property4-regression-20260506-170500`
- **PR**: TBD
- **Status**: implemented
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/conformance/jsx/checkJsxChildrenProperty4.tsx`.
The current picker reports a fingerprint-only mismatch with the expected
diagnostic codes (`TS2322`, `TS2551`) still present. PR #2812 previously fixed
this fixture, so this slice will identify the current drift or regression and
align the JSX children diagnostics without changing the diagnostic code set.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/children.rs`
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs`
- `crates/tsz-solver/src/caches/db.rs`
- `crates/tsz-solver/src/caches/query_cache.rs`
- `crates/tsz-solver/src/intern/core/interner.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --test jsx_component_attribute_tests -E 'test(jsx_react_multiple_render_prop_children_ts2322_message_preserves_react_child_alias)'`
- `./scripts/conformance/conformance.sh run --filter "checkJsxChildrenProperty4" --verbose`
- Pre-commit hook: clippy, wasm rustc warning gate, architecture guardrails, and 21,723 nextest tests passed.
