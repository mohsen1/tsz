# [WIP] fix(checker): align builtin Iterator abstract compatibility

- **Date**: 2026-04-29
- **Branch**: `fix/checker-builtin-iterator-abstract-compat`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`TypeScript/tests/cases/compiler/builtinIterator.ts` currently reports an
extra construct-signature diagnostic and misses the expected abstract-class,
abstract-member, override, and Iterator/Iterable compatibility diagnostics.
This PR investigates the root cause through the checker/solver boundary and
aligns TSZ with tsc for the builtin `Iterator` conformance case.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "builtinIterator" --verbose`
- Planned: targeted unit tests in the owning crate.
- Planned: crate-level `cargo nextest run` for touched crates.
