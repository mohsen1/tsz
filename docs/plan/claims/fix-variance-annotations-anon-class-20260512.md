# fix(checker): suppress varianceAnnotations anonymous-class extras

- **Date**: 2026-05-12
- **Branch**: `fix/variance-annotations-anon-class-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5707
- **Status**: ready
- **Workstream**: conformance

## Intent

Continue reducing `varianceAnnotations.ts` fingerprint-only drift after the TS2345 display fix. This slice targets only the two extra TS2322 diagnostics on the anonymous class repro (`InstanceType<Anon<T>>`) and leaves the remaining missing `Baz<string>` diagnostic for a separate semantic slice.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `docs/plan/claims/fix-variance-annotations-anon-class-20260512.md`

## Verification

- Baseline on current main after PR #5699: `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` passed; `tsz-conformance --filter varianceAnnotations --print-fingerprints --verbose` reported missing `Baz<string>` TS2322 plus two extra anonymous-class TS2322 fingerprints.
- After this slice: `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` passed in 59.83s.
- After this slice: `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter varianceAnnotations --print-fingerprints --verbose` reports `1/2 passed`, fingerprint-only `1`, with no extra fingerprints and only the known missing `TS2322 test.ts:117:1 Type 'Baz<string>' is not assignable to type 'Baz<unknown>'.`
