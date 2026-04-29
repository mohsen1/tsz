# fix(checker): suppress false TS2345 on generic mapped-type indexed access

- **Date**: 2026-04-29
- **Branch**: `fix/checker-mapped-generic-indexed-access-ts2345`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance — false-positive elimination)

## Intent

Eliminate the false-positive TS2345 emitted by tsz on
`conformance/compiler/mappedTypeGenericIndexedAccess.ts`. tsc accepts both
repro patterns (`this.entries[name]?.push(entry)` from #49242 and
`typeHandlers[p.t]?.(p)` from #49338); tsz incorrectly rejects one of them
with `Argument of type ... is not assignable to parameter of type ...`.

The shared shape is calling/applying through a generic indexed access on a
mapped type whose value is optional. The argument inferred from the
indexed-access result is structurally compatible with the (instantiated)
parameter, but tsz's assignability path is comparing types that haven't been
resolved through the mapped-type indexing rule, producing a spurious
mismatch.

## Plan

1. Pin the failing line and the exact source/target types via `--verbose`.
2. Trace `query_boundaries::assignability` for the call-site argument check
   and identify whether the gap is in solver indexed-access evaluation,
   in mapped-type substitution at the boundary, or in checker-side
   instantiation prior to the relation call.
3. Fix at the lowest level that preserves the architecture rules in
   `.claude/CLAUDE.md` §4–§6 (no checker pattern-matching of solver
   internals; route through `query_boundaries`).
4. Add a unit-test lock in `tsz-checker` covering both reproductions.
5. Verify net-zero conformance regression and that the false TS2345 is gone.

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "mappedTypeGenericIndexedAccess" --verbose` → 1/1 pass.
- Targeted unit tests for the two repro shapes.
- `cargo nextest run --package tsz-checker --lib` clean.
- No new conformance regressions on the sibling mapped-type tests
  (`reverseMappedTupleContext`, `mappedTypeGenericIndexedAccess` siblings).
