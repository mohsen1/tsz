# finish-pr-1381: ship `[] | [X]` rest tuple unpacking

- **Date**: 2026-04-26
- **Branch**: `fix/solver-bivariant-params-only-method-compat`
- **PR**: #1381
- **Status**: ready
- **Workstream**: 1 — Diagnostic Conformance / TS2416 false positives (foundation work)

## Takeover Notes

This file records the handover that moved PR #1381 from WIP/in-progress to
ready-for-review. The implementation work was already in place from prior
sessions (~259 LOC across solver + tests + claim file).

## Actions Taken in This Slice

1. Rebased `fix/solver-bivariant-params-only-method-compat` onto current
   `origin/main` (which had landed PR #1387's `gcp-full-ci.sh` memory cap and
   matching Cargo.toml comment). The pre-rebase diff against main showed
   spurious reverts of those CI bits because the branch was off the older
   `a0cdb8aea6` commit; rebase eliminated those phantom changes.
2. Verified the diff is now solver-only:
   - `crates/tsz-solver/src/type_queries/data/accessors.rs` (+92)
   - `crates/tsz-solver/src/tests/type_queries_function_rewrite_tests.rs` (+116)
   - `docs/plan/claims/fix-solver-unpack-tuple-rest-prefix-aligned-union.md` (+51)
3. Ran the targeted test suite: `cargo nextest run -p tsz-solver --lib
   type_queries_function_rewrite` → **6 passed, 0 failed**.
4. Ran `cargo clippy -p tsz-solver --lib --no-deps` → no warnings.
5. Updated existing claim file Status from `claim` → `ready` and stamped PR
   number.
6. Force-pushed the rebased branch.

## Architecture Compliance

- **§3 Responsibility Split**: Solver-only (`WHAT`). No checker, binder, or
  boundary helper changes. Pure type-shape transformation lives where it
  belongs.
- **§4 Hard Architecture Rules**: No new checker access to solver internals.
  No new `TypeKey` leakage. The new helper consumes the existing public
  `get_union_members` / `get_tuple_elements` accessors.
- **§11 Solver Contracts**: New helper is part of the same `accessors.rs` file
  that already owns this kind of shape destructuring. No new caches, no
  visitor-bypassing recursion.
- **§22 / §23 TS2322 Rules**: N/A. This change does not touch
  `query_boundaries/assignability` and does not introduce new
  `CompatChecker` callers. It transforms parameter shape *before* any
  relation/compat code runs.

## Verification

- `cargo nextest run -p tsz-solver --lib type_queries_function_rewrite` — 6 pass
  (3 prior + 3 new).
- `cargo clippy -p tsz-solver --lib --no-deps` — clean.
- CI on the rebased push will run the full unit + conformance + emit lanes.
- Per the PR body, the underlying conformance counts are net-zero in this
  slice; this is documented foundation work.

## Follow-up

The original `customAsyncIterator.ts` target needs the `Promise<IteratorResult<T,
any>>` vs `Promise<IteratorResult<T, void>>` return-type comparison fix to
actually flip. That is documented in the PR body and the existing claim file
under "Notes on `customAsyncIterator.ts`" and is out of scope for this slice.
