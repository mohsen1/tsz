# fix(checker): align JSDoc template variance diagnostics and callback implicit-any handling

- **Date**: 2026-05-01
- **Branch**: `fix/checker-jsdoc-template-variance-ts1274`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

This PR fixes the conformance failure in `jsdocTemplateTag8.ts` by enforcing TS1274 for JSDoc `@template in/out` on function declarations and by aligning callback contextual typing so JSDoc-typed callback parameters do not spuriously emit TS7006. The change is structural and checker-owned: diagnostics are emitted at the JSDoc modifier site and contextual-unknown handling is treated as real context in JS/checkJs callback positions.

## Files Touched

- `crates/tsz-checker/src/jsdoc/diagnostics_templates.rs`
- `crates/tsz-checker/src/state/state_checking_members/function_declaration_checks.rs`
- `crates/tsz-checker/src/types/function_type.rs`
- `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/tests/jsdoc_template_variance_function_tests.rs`
- `crates/tsz-checker/Cargo.toml`

## Verification

- `cargo nextest run -p tsz-checker --test jsdoc_template_variance_function_tests` (4 passed)
- `./scripts/conformance/conformance.sh run --filter "jsdocTemplateTag8" --verbose` (1/1 passed)
- `cargo nextest run -p tsz-checker --lib` (3070 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
