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

- Focused Rust regression for the display order, if there is an existing local
  test surface for this diagnostic.
- Filtered conformance for
  `compiler/complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound.ts` with
  `--print-fingerprints`.

## Status

Claimed on 2026-05-06 before implementation.
