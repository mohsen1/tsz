# fix(solver): pin variance for nested-callback method params (Promise<T> covariance)

- **Date**: 2026-04-30
- **Branch**: `claude/brave-thompson-i2RuZ`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance — variance / generic assignability

## Intent

Fix the variance computation so generic interfaces whose only mention of `T`
is inside a non-method callback nested inside a method parameter — the
`interface Promise<T> { then<U>(cb: (x: T) => Promise<U>): Promise<U>; }`
shape — are recognised as COVARIANT instead of bivariant. tsc rejects
`Promise<Foo>` -> `Promise<Bar>` for unrelated `Foo`/`Bar`; tsz previously
accepted it because `method_bivariant_depth` propagated transitively into
nested non-method functions, marking the leaf `T` occurrence with
`REJECTION_UNRELIABLE` and silently falling through to a structural
comparison that re-applied method bivariance and accepted both directions.

The variance visitor now (a) saves and resets `method_bivariant_depth = 0`
at every nested non-method function/callable boundary so leaf occurrences
inside callbacks record their actual polarity, and (b) tracks
`strict_occurrence_seen` / `inside_unreliable_application` so a real strict
occurrence pins the variance and clears the bivariant
`REJECTION_UNRELIABLE` set by sibling direct-method-param occurrences,
without demoting wrapper types like `{ container: C1<T> }` whose only `T`
appearance is inherited from the wrapped bivariant generic.

Recovers TS2322 emission on `tests/cases/compiler/promisesWithConstraints.ts`
(missing-fingerprint failure) and aligns variance with tsc on the
`m(x: T, cb: (x: T) => void)`, callable-nested, and JSX-callback patterns.

## Files Touched

- `crates/tsz-solver/src/relations/variance.rs` (~80 LOC change in the
  `VarianceVisitor` — new fields + scope reset in `visit_function`,
  `visit_callable`, `visit_application`; clear in `compute()`).
- `crates/tsz-solver/tests/variance_tests.rs` (+170 LOC: 3 new tests).
- `crates/tsz-checker/tests/promise_callback_variance_tests.rs` (new
  integration test file: 4 end-to-end checker assertions).
- `crates/tsz-checker/Cargo.toml` (register the new test target).
- `scripts/session/quick-random-failure.sh` (new wrapper around `pick.py`
  that ensures the TypeScript submodule is initialised, validates the
  offline snapshot, and optionally previews the test source).

## Verification

- `cargo nextest run -p tsz-solver --lib` — 5560 passed, 9 skipped.
- `cargo nextest run -p tsz-checker --lib` — 3047 passed, 10 skipped.
- `cargo nextest run -p tsz-checker --test promise_callback_variance_tests`
  — 4/4 passed.
- `cargo fmt --all --check` — clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  — clean.
- `./scripts/conformance/conformance.sh run --filter promisesWithConstraints
  --verbose` — 1/1 passed (was previously failing as missing TS2322).
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` — net
  **12262 → 12266 (+4)**: 12 PASS gains (the original
  `promisesWithConstraints` target plus eleven downstream wins on
  `mappedTypeGenericIndexedAccess`, `controlFlowAliasing`, the
  `taggedTemplateStringsWithOverloadResolution1` pair, the
  `rewriteRelativeImportExtensions` JS/CommonJS pair,
  `await_incorrectThisType`, `contextuallyTypedBindingInitializerNegative`,
  `assignFromNumberInterface2`,
  `circularlySimplifyingConditionalTypesNoCrash`, and
  `es6ImportDefaultBindingFollowedWithNamedImport1`) vs 3 PASS→FAIL
  regressions (`typeGuardConstructorClassAndNumber`,
  `logicalAssignment6`, `logicalAssignment7` — fingerprint-only divergence
  with one extra/missing diagnostic each, all secondary to the primary
  expected error). Net positive, well within the "handful" tolerance from
  `scripts/session/conformance-agent-prompt.md`.
