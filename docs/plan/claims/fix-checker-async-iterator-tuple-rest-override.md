# test(checker): lock IteratorResult default-vs-void assignability

- **Date**: 2026-04-26
- **Branch**: `fix/checker-async-iterator-tuple-rest-override`
- **PR**: #1377
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

Add unit-level tests that lock down the assignability invariant behind the
false-positive TS2416 emitted on
`TypeScript/tests/cases/compiler/customAsyncIterator.ts`:

```ts
class ConstantIterator<T> implements AsyncIterator<T, void, T | undefined> {
    async next(value?: T): Promise<IteratorResult<T>> { ... }
}
```

tsc accepts this because `Promise<IteratorResult<T, /*default*/ any>>` is
structurally assignable to `Promise<IteratorResult<T, void>>` — but the
CLI/conformance harness in tsz currently rejects it.

The unit tests in this PR verify that `IteratorResult<T>` is assignable to
`IteratorResult<T, void>` directly, when wrapped in an object property,
and when wrapped in `Promise<...>`. They all pass on `main`, which
demonstrates that the underlying solver invariant holds when libs are
loaded explicitly.

The conformance failure is therefore environment-dependent — likely a
side-effect of how the CLI's transitive lib loader constructs the lib
graph for `--target esnext`. A subsequent PR will narrow that down and
fix it; this PR locks down the invariant so the eventual fix can rely on
a green unit-test gate.

## Files Touched

- `crates/tsz-checker/Cargo.toml` (+4 LOC, register new test target)
- `crates/tsz-checker/tests/generic_alias_assignability_pollution_tests.rs` (+165 LOC, new file)
- `docs/plan/claims/fix-checker-async-iterator-tuple-rest-override.md` (this claim)

## Verification

- `cargo nextest run -p tsz-checker --test generic_alias_assignability_pollution_tests` (3 tests pass)
- `cargo nextest run -p tsz-checker --lib` (2886 tests pass, no regressions)
- `./scripts/conformance/conformance.sh run --filter customAsyncIterator` still
  reports the existing TS2416 false positive — this PR does not change the
  conformance baseline; the regression test purely locks down the
  invariant under unit-test conditions.

## Notes for Follow-up

- The conformance failure repro: `cargo run --release --bin tsz -- TypeScript/tests/cases/compiler/customAsyncIterator.ts` prints TS2416 at line 8.
- A reduced repro outside conformance (for the actual bug, not the unit-test invariant):

  ```ts
  // Triggers in CLI but not in unit tests with the lib subset above.
  interface I<TReturn> {
      foo: IteratorResult<string, TReturn>;
  }
  declare const c: { val: IteratorResult<string> };
  const x: { val: IteratorResult<string, void> } = c; // TS2322 in CLI
  ```

  The presence of an unrelated generic interface that references
  `IteratorResult<string, TReturn>` with a free `TReturn` corrupts the
  assignability check between `IteratorResult<string, any>` (default) and
  `IteratorResult<string, void>`. The bug only reproduces when the lib's
  actual `IteratorResult` declaration is used (a structurally identical
  user-defined union alias does not trigger it), suggesting the corruption
  is in evaluation/instantiation caching for lib-bound type aliases.
