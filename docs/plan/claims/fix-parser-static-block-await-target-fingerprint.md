# [WIP] fix(parser): align static-block await target diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/parser-static-block-await-target-fingerprint`
- **PR**: #1831
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Align the remaining `classStaticBlock26.ts` diagnostic fingerprints around
`await` inside class static blocks. The prior static-block parser follow-up
fixed reserved-word handling but left target-dependent `await (1)` behavior as
a known limitation because the parser did not know the script target. This PR
will diagnose the root cause, route any target-sensitive decision through the
right parser/checker boundary, and keep the fix scoped to this conformance
shape.

## Files Touched

- `crates/tsz-parser/src/parser/` (expected)
- `crates/tsz-checker/src/` or compiler options plumbing if target context is
  required (expected)

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "classStaticBlock26" --verbose`
- Planned: owning-crate unit tests with `cargo nextest run`
