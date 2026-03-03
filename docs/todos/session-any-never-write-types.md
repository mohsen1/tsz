# Session: any→never Assignability & Union-Keyed Write Types

## Summary
Fixed two interrelated issues that prevented `intersectionReductionStrict.ts` from passing:
1. `any → never` assignability was incorrectly allowed at multiple layers
2. Union-keyed index access didn't compute write types (intersection) vs read types (union)

## Root Cause Analysis

### Problem: `x1[k] = 'bar' as any` where `x1: {a: string, b: number}` and `k: 'a' | 'b'`
- **Expected**: TS2322 — write type is `string & number = never`, and `any → never` is an error
- **Actual**: No error — two independent bugs prevented the diagnostic

### Bug 1: Write types not computed for union-keyed index access
When `obj[k]` is an assignment target with `k: 'a' | 'b'`, the type should be:
- **Read context**: `string | number` (union — value could be any property type)
- **Write context**: `string & number` (intersection — value must satisfy ALL property types)

The checker always returned the union (read type). Fixed by checking `skip_flow_narrowing` flag
(set by `get_type_of_assignment_target`) and returning intersection in write context.

### Bug 2: `any → never` incorrectly allowed at FOUR layers
TypeScript's `any` bypasses most type checks but is NOT assignable to `never`.
The fix required changes at every layer that handled `any`:

1. **Solver subtype fast path** (`cache.rs:84`): `any` was unconditionally assignable to everything
2. **Compat checker fast path** (`compat.rs:1032`): Lawyer layer had its own `any` suppression via `allow_any_suppression`
3. **Assignability diagnostic suppression** (`assignability_checker.rs:190`): Blanket-suppressed all diagnostics when source is `any`
4. **Error reporter fallback** (`error_reporter/assignability.rs:233`): Generic error reporting also suppressed `any` sources

All four needed the same fix: exclude `target == TypeId::NEVER` from the `any` bypass.

## Files Changed
- `crates/tsz-solver/src/relations/subtype/cache.rs` — Subtype fast path: `any → T` now excludes `T = never`
- `crates/tsz-solver/src/relations/compat.rs` — Compat fast path: same exclusion
- `crates/tsz-checker/src/assignability/assignability_checker.rs` — Diagnostic suppression: allow `any → never`
- `crates/tsz-checker/src/error_reporter/assignability.rs` — Generic error reporting: same
- `crates/tsz-checker/src/types/computation/access.rs` — Union-key combining: intersection in write context
- `crates/tsz-checker/src/types/utilities/core.rs` — Literal key access: intersection in write context, use `write_type` from PropertyAccessResult
- `crates/tsz-checker/src/assignability/assignment_checker.rs` — New unit tests
- `crates/tsz-checker/tests/any_propagation_tests.rs` — Updated `any → never` and `any → string & number` tests

## Impact
- **Conformance**: +4 tests (9825 → 9829, 78.2%)
- **Tests affected**: `intersectionReductionStrict.ts` and 3 other tests that involved `any → never` assignments
- **No regressions**: All 2556 existing tests pass

## Key Architectural Insight
The Judge/Lawyer model means `any` handling exists at TWO levels:
- Judge (SubtypeChecker): `check_subtype` in `cache.rs`
- Lawyer (CompatChecker): `check_assignable_fast_path` in `compat.rs`

The Lawyer short-circuits BEFORE the Judge, so fixing only the Judge's fast path has no effect.
Additionally, the Checker has its own diagnostic suppression layers that independently filter errors.
Any fix involving `any` semantics must audit all four layers.
