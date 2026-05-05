# fix(checker): align control flow array diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-control-flow-array-errors-fingerprint`
- **PR**: #2853
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint-only
  `controlFlowArrayErrors.ts`)

## Intent

Align tsz diagnostic fingerprints for
`TypeScript/tests/cases/compiler/controlFlowArrayErrors.ts` with tsc. The
remaining drift was semantic: evolving arrays updated by `push` preserved the
antecedent array type in flow, so later reads did not snapshot element growth.
That missed the expected `TS2345` at an alias read after a later push and at a
branch merge with a non-evolving array branch.

Minimal repro:

```ts
function f(cond: boolean) {
    let x;
    if (cond) {
        x = [];
        x.push(5);
        x.push("hello");
    } else {
        x = [true];
    }
    x.push(99); // TS2345: 99 is not assignable to never
}
```

## Files Touched

- `crates/tsz-checker/src/flow/control_flow/core.rs`
- `crates/tsz-checker/src/types/computation/identifier/core.rs`
- `crates/tsz-checker/src/types/computation/identifier_flow.rs`
- `crates/tsz-checker/tests/conformance_issues/features/implicit_any.rs`

## Verification

- `cargo fmt --all --check` — pass.
- `cargo check --package tsz-checker` — pass.
- `cargo nextest run --package tsz-checker --test conformance_issues -E 'test(test_evolving_array_read_snapshots_before_later_push) + test(test_evolving_array_branch_merge_with_non_evolving_array_rejects_push)'` — 2 pass.
- `./scripts/conformance/conformance.sh run --filter "controlFlowArrayErrors" --verbose` — 1/1 pass.
- `./scripts/conformance/conformance.sh run --max 200` — 200/200 pass.
