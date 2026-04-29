# fix(checker): per-property TS2322 elaboration for destructuring var declarations

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-1qlf9`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

When a destructuring variable declaration has a type annotation and an object-literal initializer
with mismatching property values (e.g. `var {a1, a2}: { a1: number, a2: string } = { a1: true, a2: 1 }`),
tsc reports TS2322 per mismatching property inside the RHS object literal (at the property key),
not a single error at the binding pattern. This PR adds that per-property elaboration path in the
`is_destructuring` branch of `check_variable_declaration_with_request`, mirroring the existing
`try_elaborate_object_literal_properties_for_var_init` path used for non-destructuring declarations.

Fixes `TypeScript/tests/cases/conformance/es6/destructuring/destructuringVariableDeclaration2.ts`
going from fingerprint-only to fully passing.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs` (~20 LOC change)
- `crates/tsz-checker/tests/ts2322_destructuring_obj_literal_tests.rs` (new, 65 LOC)
- `crates/tsz-checker/src/lib.rs` (~3 LOC, test module registration)

## Verification

- `cargo test -p tsz-checker --lib ts2322_destructuring_obj_literal` (3 tests pass)
- `./scripts/conformance/conformance.sh run --filter "destructuringVariableDeclaration2"` (1/1 pass)
- Destructuring conformance area: 162/174 (vs 161/174 baseline — +1 net gain)
