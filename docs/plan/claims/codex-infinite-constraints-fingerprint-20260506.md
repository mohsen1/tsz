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

- Focused Rust regression for the changed diagnostic path.
- Filtered conformance for `compiler/infiniteConstraints.ts` with
  `--print-fingerprints`.

## Status

Claimed on 2026-05-06 before implementation.
