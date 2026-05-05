# fix(checker): preserve implicit-any diagnostic on complex contextual call

- **Date**: 2026-05-05
- **Branch**: `fix/checker-normalized-intersection-too-complex-implicit-any`
- **PR**: #2847
- **Status**: ready for review
- **Workstream**: 1 (Conformance — missing TS7006 in
  `normalizedIntersectionTooComplex.ts`)

## Intent

Restore the missing TS7006 for the unannotated arrow parameter in
`TypeScript/tests/cases/compiler/normalizedIntersectionTooComplex.ts` and align
the TS2590 span with tsc.

## Initial Signal

```
path:     TypeScript/tests/cases/compiler/normalizedIntersectionTooComplex.ts
category: only-missing
expected: TS2590,TS7006
actual:   TS2590
missing:  TS7006
extra:    -
pool:     8
```

Verbose current-main diff:

```
missing:
  TS2590 test.ts:37:40 Expression produces a union type that is too complex to represent.
  TS7006 test.ts:37:40 Parameter 'x' implicitly has an 'any' type.
extra:
  TS2590 test.ts:37:14 Expression produces a union type that is too complex to represent.
```

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "normalizedIntersectionTooComplex" --verbose`
- Targeted Rust regression test in the owning crate for the root cause.
- `cargo fmt --all --check`
- Targeted crate check/test commands for changed code.
- `./scripts/conformance/conformance.sh run --filter "normalizedIntersectionTooComplex"`
- `./scripts/conformance/conformance.sh run --max 200`

## Implementation

- Anchor TS2590 from an over-complex call context on the first unannotated
  callback parameter found in direct or object/array-literal call arguments.
- Emit the paired TS7006 at the same callback parameter when `noImplicitAny` is
  enabled.
- Added a Rust regression covering the `normalizedIntersectionTooComplex.ts`
  repro shape.

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker`
- `cargo test --package tsz-checker --lib ts2590_and_ts7006_anchor_to_nested_callback_param_when_context_too_complex`
- `./scripts/conformance/conformance.sh run --filter "normalizedIntersectionTooComplex" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
