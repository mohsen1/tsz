# [WIP] fix(checker): align mapped recursive XMLHttpRequest fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-mapped-recursive-xhr-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / fingerprint parity

## Intent

Random conformance pick selected
`TypeScript/tests/cases/compiler/mappedTypeRecursiveInference.ts`.
The test is fingerprint-only: `tsc` and `tsz` both emit `TS2345`, but the
displayed target for `Deep<XMLHttpRequest>` orders and expands properties
differently. This PR will root-cause the display/inference surface needed to
match `tsc` without hardcoding the conformance file.

Observed verbose mismatch on `origin/main`:

- Missing fingerprint: `Deep<{ onreadystatechange: unknown; readonly readyState: { toString: ...; ... }; readonly response: unknown; readonly responseText: { toString: ...; ... 39 more ...; [Symbol.iterator]: ...; }; ... 29 more ...; dispatchEvent: unknown; }>`
- Extra fingerprint: `Deep<{ dispatchEvent: unknown; onerror: unknown; addEventListener: unknown; onload: unknown; readonly status: unknown; open: unknown; onabort: unknown; removeEventListener: unknown; responseType: unknown; readonly responseURL: unknown; ... 23 more ...; readonly readyState: unknown; }>`

## Files Touched

- TBD after root-cause analysis; likely checker diagnostic type display and
  owning crate regression tests.

## Verification

- `./scripts/conformance/conformance.sh run --filter "mappedTypeRecursiveInference" --verbose` (currently fingerprint-only, baseline captured)
- Planned: `cargo check --package tsz-checker`
- Planned: owning-crate `cargo nextest run` for any new regression tests
- Planned: targeted conformance rerun for `mappedTypeRecursiveInference`
- Planned: quick conformance regression sample before marking ready
