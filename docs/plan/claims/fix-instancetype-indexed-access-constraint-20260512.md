# fix(checker): suppress cascading InstanceType constraint diagnostic

- **Date**: 2026-05-12
- **Branch**: `fix-instancetype-indexed-access-constraint-20260512`
- **Base**: `main`
- **Issue**: [#6093](https://github.com/mohsen1/tsz/issues/6093)
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (diagnostic conformance / false-positive checker bug)

## Intent

Make `tsz` match `tsc` when `InstanceType<Outer["Inner"]>` first reports
TS2749 because `Outer` is a value used as a type. The checker should not also
emit the cascading TS2344 constraint diagnostic for the invalid type argument.

## Planned Scope

- Add a focused checker regression for the #6093 repro.
- Suppress the extra constraint check only when the type argument is already
  invalid or unresolved.
- Preserve real TS2344 diagnostics for valid but constraint-incompatible
  `InstanceType` arguments.

## Verification Plan

- Targeted checker test for the #6093 repro.
- Focused TS2344 / type-argument checker tests.
- `env CARGO_INCREMENTAL=0 cargo check -p tsz-checker`
- Manual #6093 repro comparison against `tsc` and `tsz`.
