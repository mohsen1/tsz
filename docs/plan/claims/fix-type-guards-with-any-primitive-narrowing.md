# fix(checker): align typeof primitive narrowing for any

- **Status**: ready
- **Branch**: `fix/type-guards-with-any-primitive-narrowing`
- **Workstream**: 1 (Conformance fallback after checking Workstream 5 for an obvious small slice)

## Scope

Fix the fingerprint-only mismatch in
`TypeScript/tests/cases/conformance/expressions/typeGuards/typeGuardsWithAny.ts`.

`tsc` narrows `any` in true branches for primitive `typeof` checks
(`"string"`, `"number"`, `"boolean"`) but leaves `any` unaffected by
`instanceof`, false branches, and the `"object"` check in this test. Current
`tsz` already emits the right `TS2339` code set, but reports the wrong
branches/types for number, boolean, object, and else branches.

## Verification Plan

- Add a focused checker regression for `typeof` primitive narrowing from `any`.
- `cargo test -p tsz-checker <focused test> -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "typeGuardsWithAny" --verbose`
- `cargo fmt --check`
- `git diff --check`

## Verification

- `cargo test -p tsz-checker --test control_flow_type_guard_tests typeof_primitive_checks_narrow_explicit_any_only_in_true_branch -- --nocapture`
- `./scripts/conformance/conformance.sh run --filter "typeGuardsWithAny" --verbose`
- `cargo fmt --check`
- `git diff --check`
