# fix(checker): realign mapped type error fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-mapped-type-errors-fingerprint`
- **PR**: #2913
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint-only `mappedTypeErrors.ts`)

## Intent

Realign diagnostic fingerprints for
`TypeScript/tests/cases/conformance/types/mapped/mappedTypeErrors.ts` with
tsc. The current drift was two extra `TS2322` diagnostics for
`Pick<T, K>` object literals that explicitly write `undefined` to an optional
property while `exactOptionalPropertyTypes` is disabled.

Root cause: the mapped-object literal fast path stopped at the first
per-property target type, often the stripped assignment-side `number`, and did
not account for the contextual target property being optional. That made
`{ b: undefined }` fail against `Pick<Foo, "b">` even though tsc accepts it
when `Foo["b"]` comes from `b?: number`.

Minimal repro:

```ts
interface Foo {
    a: string;
    b?: number;
}
declare function setState<T, K extends keyof T>(obj: T, props: Pick<T, K>): void;
let foo: Foo = { a: "hello", b: 42 };
setState(foo, { b: undefined }); // OK without exactOptionalPropertyTypes
```

This fixture has a historical merged claim in #1832; this claim tracks the
current `origin/main` fingerprint drift selected by `quick-pick.sh`.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/mapped_object_literals.rs`
- `crates/tsz-checker/src/state/state_checking/property.rs`
- `crates/tsz-checker/tests/mapped_type_errors_conformance_tests.rs`

## Verification

- `cargo fmt --all --check` — pass.
- `cargo check --package tsz-checker` — pass.
- `cargo check --package tsz-solver` — pass.
- `cargo nextest run --package tsz-checker --test mapped_type_errors_conformance_tests` — 5 pass.
- `./scripts/conformance/conformance.sh run --filter "mappedTypeErrors" --verbose` — 2/2 pass.
- `./scripts/conformance/conformance.sh run --max 200` — 200/200 pass.
