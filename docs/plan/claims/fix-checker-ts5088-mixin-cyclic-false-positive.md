# fix(checker): suppress false TS5088 on mixin anonymous-class return type

- **Date**: 2026-04-29
- **Branch**: `fix/checker-ts5088-mixin-cyclic-false-positive`
- **PR**: #1813
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — false-positive elimination)

## Intent

Eliminate the false-positive TS5088 emitted on
`conformance/classes/mixinAccessors1.ts` at line 8 (the `mixin`
function). tsc accepts the inferred return type
`(superclass: T) => { new (...args: any[]): { validationTarget: HTMLElement; } } & T`;
tsz incorrectly reports
"The inferred type of 'mixin' references a type with a cyclic structure
which cannot be trivially serialized."

The emit point is gated by
`declaration_type_references_cyclic_structure`
(`crates/tsz-solver/src/type_queries/traversal.rs:335`). The fix is in
the cycle-detection traversal — it currently reports a cycle for the
mixin's anonymous-class return type even though the cycle resolves
through a named reference (T's constraint or the inferred class shape)
that the declaration emitter can serialize without inlining.

## Plan

1. Pin which traversal branch returns true for this case (Application,
   Recursive, Lazy, or Conditional).
2. Tighten the condition so the mixin-style return type doesn't trip
   it. Likely candidates: the `application_contains_nonserializable_recursive_alias`
   helper, or the conditional-branch propagation of `in_cond_branch`
   through structural members.
3. Add a unit test pinning the behavior.
4. Verify net-zero conformance regression.

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "mixinAccessors1" --verbose` → 1/1 pass.
- New unit test in `tsz-checker` (or `tsz-solver`).
- `cargo nextest run -p tsz-checker -p tsz-solver` clean.
- No new conformance regressions.
