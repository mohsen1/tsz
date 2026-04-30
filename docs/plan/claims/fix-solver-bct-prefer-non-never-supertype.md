# fix(solver): keep BCT's non-`never` return-position result when the only concrete bound is `never`

- **Date**: 2026-04-30
- **Branch**: `fix/solver-bct-prefer-non-never-supertype`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

`subtypeRelationForNever.ts` (TS issue #51999) expects `withFew([1,2,3], id, fail)` to type-check cleanly:

```ts
function fail(message: string): never { throw new Error(message); }
function withFew<a, r>(values: a[], haveFew: (values: a[]) => r, haveNone: (reason: string) => r): r { … }
function id<a>(value: a): a { return value; }
const result = withFew([1, 2, 3], id, fail); // expected: number[]
```

`r` collects two return-position covariant candidates: `never` (from `fail`) and `unknown`/`number[]` (from `id`'s body and the contextual return). `get_common_supertype_for_inference` already filters `never` out before BCT, so the chosen `inferred` is the non-`never` value. The bug was the *next* step: `resolve_return_position_inference_type` saw `inferred = unknown` and `concrete_bounds = [never]` (everything else was `unknown`/`any`/error and got filtered in the bound-collection pass) and promoted that lone `never` back into the result — forcing the downstream argument check for `id` to reject `<a>(value: a) => a` against `(values: …) => never`.

The fix is targeted: in `resolve_return_position_inference_type`, skip the "single concrete bound" promotion **only** when

1. that lone concrete bound is `never`, AND
2. the original `lower_bounds` set also carried an `any` / `unknown` candidate (so BCT had something better to pick).

Pure single-`never` lower-bound sets — e.g. `f1<T>([])` where the only signal is "no information" — keep their existing `T = never` resolution (`only_never_candidates_resolves_to_never` test guards this).

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/inference_helpers.rs` — guard the `never`-promotion branch in `resolve_return_position_inference_type` (≈20 LOC including rationale comment).
- `crates/tsz-checker/tests/co_contra_inference_tests.rs` — added `never_return_candidate_does_not_force_never_inference` regression test using the exact `withFew` shape from the conformance case.

## Verification

- `cargo nextest run -p tsz-checker --test co_contra_inference_tests` — 5 tests pass; both the new regression test and the pre-existing `only_never_candidates_resolves_to_never` counter-test stay green.
- `cargo nextest run -p tsz-checker -p tsz-solver` — 11285 tests pass.
- `bash scripts/conformance/conformance.sh run --workers 8` — net **+11**: 13 improvements (`subtypeRelationForNever.ts` flips FAIL → PASS, plus 12 sibling tests that share the BCT pipeline). The 2 listed `PASS → FAIL` entries (`circularInlineMappedGenericTupleTypeNoCrash.ts`, `typeGuardConstructorClassAndNumber.ts`) are stale-baseline artifacts: both are FAIL on `origin/main` without this fix as well; the committed `conformance-baseline.txt` simply hadn't been refreshed since the upstream regression.
