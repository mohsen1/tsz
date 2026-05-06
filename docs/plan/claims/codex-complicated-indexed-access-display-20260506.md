# Claim: Stabilize complicated indexed access union display

## Target

`TypeScript/tests/cases/compiler/complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound.ts`

Current conformance mismatch is fingerprint-only: tsz reports the expected TS2322
at the correct location, but the displayed union members in the target type are
reversed:

```text
expected: NewChannel<ChannelOfType<T, TextChannel> | ChannelOfType<T, EmailChannel>>
actual:   NewChannel<ChannelOfType<T, EmailChannel> | ChannelOfType<T, TextChannel>>
```

## Plan

Find where union constituents are ordered for diagnostic display in the indexed
access / keyof-never path, then preserve or sort the display order so the
fingerprint matches TypeScript without changing assignability behavior.

## Verification

- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test ts2322_tests
  test_ts2322_keeps_outer_object_error_for_direct_index_access_target -- --exact
  --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-solver diagnostics::format --
  --nocapture`
- Filtered conformance for
  `compiler/complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound.ts` with
  `--print-fingerprints`: 1/1 passed, fingerprint-only 0.

## Status

Implemented and verified on 2026-05-06.
