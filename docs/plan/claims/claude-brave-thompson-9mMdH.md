# fix(solver): preserve array shape for `any`-substituted homomorphic mapped types with non-identity templates

- **Date**: 2026-04-26
- **Branch**: `claude/brave-thompson-9mMdH`
- **PR**: TBD
- **Status**: ready
- **Workstream**: Conformance — fingerprint parity / wrong-code reduction

## Intent

Fixes `instantiateMappedType` parity with tsc when a homomorphic mapped type
`{ [K in keyof T]: V }` is instantiated with `T = any` and `T` is constrained
to an array/tuple type (including readonly arrays/tuples). Previously the
array-preservation path required the template to reference `T[K]`; templates
whose body did not reference `T[K]` (e.g. `string`) leaked through and produced
a `{ [x: string]: V; [x: number]: V }` object instead of `V[]`.

This matches tsc's `instantiateMappedType` branch:

```ts
if (isArrayType(t) || (t.flags & TypeFlags.Any && constraint && everyType(constraint, isArrayOrTupleType))) {
    return instantiateMappedArrayType(...);
}
```

Adds the `any-with-array-constraint` case as an early check that runs even
when the template does not reference `T[K]`. Existing identity-template paths
(`Arrayish<any>` returning `any`) and the existing block that requires
`mapped_template_uses_source_index` are unchanged.

## Files Touched

- `crates/tsz-solver/src/instantiation/instantiate.rs` (~60 LOC: new early-check branch)
- `crates/tsz-solver/tests/instantiate_tests.rs` (+73 LOC: regression test)
- `scripts/session/pick.sh` (new file: one-shot random failure picker with source preview)

## Verification

- `cargo nextest run --package tsz-solver --lib` — 5392 pass
- `cargo nextest run --package tsz-checker --lib` — 2774 pass
- `cargo fmt --all --check` — clean
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `scripts/session/verify-all.sh --quick` — conformance improves by +9 tests
  (10 flipped to PASS, none regressed).
