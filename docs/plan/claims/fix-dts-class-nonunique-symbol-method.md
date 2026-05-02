# [WIP] fix(emitter): preserve symbol indexer method declarations

- **Date**: 2026-05-02
- **Branch**: `fix/dts-class-nonunique-symbol-method`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 2 (declaration emit pass rate)

## Intent

Investigate and fix the declaration emit mismatch for
`classNonUniqueSymbolMethodHasSymbolIndexer`. The target is a narrow emitter
change that preserves TypeScript-compatible declaration output for classes
using symbol indexers and non-unique symbol methods without broad printer
string heuristics.

## Files Touched

- TBD after focused repro.

## Verification

- Focused emit repro for `classNonUniqueSymbolMethodHasSymbolIndexer`.
