# fix(checker): evaluate `Awaited<X>` after Promise unwrap in async return-type checking

- **Date**: 2026-05-02
- **Branch**: `fix/async-return-evaluate-awaited`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — TS2322 fingerprint quality for async function returns)

## Intent

The lib signature `Promise.resolve<T>(value: T): Promise<Awaited<T>>` produces
a return type whose inner argument is an `Awaited<T>` alias application. tsz's
`unwrap_async_return_type_for_body` strips one `Promise<...>` wrapper but
leaves the inner `Awaited<X>` as a raw `TypeApplication`. The assignability
gateway then renders TS2322 source-type displays as
`Type 'Awaited<{ x: string; }>' is not assignable to ...` instead of tsc's
`Type '{ x: string; }' is not assignable to ...` (tsc's `getAwaitedType`
resolves Awaited eagerly for non-thenable inner types).

Fix: after the Promise unwrap, if the result is an `Awaited<X>` application
(detected by walking the application's base to a `Lazy(DefId)` whose
escaped name is `"Awaited"`), evaluate it via `evaluate_application_type` so
the conditional-type machinery folds it to `X`. Other generic applications
(`Box<T>`, `Partial<T>`, etc.) are left in alias form to preserve tsc's
preferred display.

This is a fingerprint/display improvement, not a logic change — assignability
relations between `Awaited<X>` and their structural form already worked
through the solver's evaluation cache; this fix routes the result through
the printer in canonical form.

Note: the targeted conformance test
`generatorReturnContextualType.ts` does not flip on this fix alone — it
remains fingerprint-only because of a separate contextual-literal-narrowing
bug for inline object literals passed through `Promise.resolve(<literal>)`
(tsc preserves the literal type via fresh-literal-type contextual flow; tsz
widens). That is a Workstream 1 follow-up.

## Files Touched

- `crates/tsz-checker/src/checkers/promise_checker.rs` (+38, -3) —
  add `is_awaited_application` and `evaluate_awaited_application` helpers,
  call the latter on the Promise-unwrap result.
- `crates/tsz-checker/tests/async_return_widening_tests.rs` (+96) —
  two new tests with inline `Awaited`/`PromiseConstructor`/`AsyncGenerator`
  prelude. Two name choices (`AsyncGenerator` + `{x:"x"}` and `AsyncIterator` +
  `{y:"y"}`) so the fix is not name-hardcoded.

## Verification

- `cargo nextest run -p tsz-checker --test async_return_widening_tests` —
  5 tests pass (3 existing + 2 new).
- `cargo nextest run -p tsz-checker -E 'test(async)|test(promise)|test(await)|test(generator)'` —
  227 tests pass.
- `cargo nextest run -p tsz-checker --lib` — 3153 tests pass.
- Targeted conformance: `./scripts/conformance/conformance.sh run --filter
  "generatorReturnContextualType" --verbose` confirms TS2322 source-type
  displays no longer contain `Awaited<`. Test remains fingerprint-only due
  to a separate contextual-narrowing bug.
- Full conformance: identical net delta (`12344 → 12346`) with and without
  the fix on the same `main`, confirming the fix is conformance-neutral
  (no regressions). The +2 / 6-up / 4-down deltas are pre-existing snapshot
  drift unrelated to this PR.

## Notes

The follow-up needed to fully flip `generatorReturnContextualType.ts` lives
in the generic-call-inference path: tsc's fresh-literal-type contextual
narrowing for inline object literals threaded through `Promise.resolve(T):
Promise<Awaited<T>>` preserves the literal type when the outer return
context is itself a literal type. tsz widens. Fixing that requires changes
in the call-inference machinery (not just the async-return wrapper) and is
out of scope for this PR.
