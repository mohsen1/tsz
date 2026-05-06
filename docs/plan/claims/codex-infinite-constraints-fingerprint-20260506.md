# Claim: Align infinite constraints TS2322 fingerprints

## Target

`TypeScript/tests/cases/compiler/infiniteConstraints.ts`

Current conformance mismatch is fingerprint-only on TS2322. The expected and
actual error codes match, but tsz reports an extra assignment failure at line 15
and misses the two `Value<"dup">` failures at line 32:

```text
missing: TS2322 test.ts:32:43 Type 'Value<"dup">' is not assignable to type 'never'.
missing: TS2322 test.ts:32:63 Type 'Value<"dup">' is not assignable to type 'never'.
extra:   TS2322 test.ts:15:20 Type '{ a: string; }' is not assignable to type 'never'.
```

## Plan

Reproduce the fixture locally, identify whether the mismatch is caused by
constraint reduction, contextual display, or relation reporting location, then
add a focused regression that preserves the TypeScript fingerprint without
weakening assignability.

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test ts2322_tests test_ts2322_infinite_constraints_duplicate_value_fingerprints -- --exact --nocapture`
- `.target/debug/tsz-conformance --test-dir <tmp> --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/debug/tsz --workers 1 --verbose --print-fingerprints --no-batch --timeout 60`

## Status

Claimed on 2026-05-06 before implementation.

Resolved on 2026-05-06 by:

- skipping premature generic constraint validation for constraints containing
  `infer`, which removes the false line 15 TS2322;
- synthesizing a more specific expected object shape for duplicate
  single-argument application display aliases when a broad object constraint
  has only `any` properties, which restores the two line 32 `Value<"dup">`
  TS2322 fingerprints;
- adding a focused TS2322 regression for the `ensureNoDuplicates` case.
