# fix(checker): preserve literal union constraint display in maxConstraints

- **Branch**: `fix/checker-max-constraints-literal-fingerprint`
- **Status**: Claimed
- **Workstream**: 1 (Diagnostic conformance and fingerprints)
- **Target**: `TypeScript/tests/cases/compiler/maxConstraints.ts`

## Intent

Fix the fingerprint-only TS2345 mismatch in `maxConstraints.ts`.

`tsc` reports:

`Argument of type 'number' is not assignable to parameter of type 'Comparable<1 | 2>'.`

`tsz` currently reports the same code and span, but displays:

`Argument of type 'number' is not assignable to parameter of type 'Comparable<number>'.`

## Scope

- Preserve the inferred literal-union constraint display for generic calls like
  `max2(1, 2)` where the target parameter is `T extends Comparable<T>`.
- Add focused checker/solver regression coverage for the display behavior.
- Keep the fix narrow to generic call inference/diagnostic formatting; no parser,
  binder, or emitter changes are expected.

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker --package tsz-solver`
- `cargo nextest run --package tsz-checker --lib`
- `./scripts/conformance/conformance.sh run --filter "maxConstraints" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
