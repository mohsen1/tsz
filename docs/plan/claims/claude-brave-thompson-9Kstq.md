# fix(checker): NUIA value-level access for any-typed and generic-key indices

- **Date**: 2026-05-03
- **Branch**: `claude/brave-thompson-9Kstq`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance) — `noUncheckedIndexedAccess.ts` (fingerprint-only → PASS)

## Intent

Fix the `noUncheckedIndexedAccess` conformance failure on
`conformance/pedantic/noUncheckedIndexedAccess.ts` (3 missing fingerprints
under one root cause family). The test exercises two structural rules that
tsz was skipping on the value-level element-access path:

1. **`obj[anyExpr]` value-level access**: an `any`-typed index expression
   must still flow through the receiver's applicable index signature, so
   NUIA widens reads to `T | undefined` and rejects writes of `undefined`
   against the un-widened slot type. Type-level `T[any] = any` is unchanged
   (a separate evaluator path).

2. **`obj[K]` write with `K extends keyof <obj>`**: when the index is a
   generic key bound to `keyof` of a *concrete* receiver, tsc preserves the
   deferred `IndexAccess(receiver, K)` form for the WRITE target so the
   assignability gate rejects `undefined` writes and surfaces the
   `Receiver[K]` display in TS2322. tsz already handled the dual
   "generic receiver with keyof-mentioning index" case; this PR adds the
   concrete-receiver counterpart.

## Files Touched

- `crates/tsz-solver/src/operations/property_helpers.rs` (~25 LOC, new
  `resolve_any_index_access` method on `PropertyAccessEvaluator`).
- `crates/tsz-solver/src/caches/db.rs` (~25 LOC, new
  `QueryDatabase::resolve_any_index_access` trait method + `TypeInterner`
  override).
- `crates/tsz-solver/src/caches/query_cache.rs` (~10 LOC, `QueryCache`
  override).
- `crates/tsz-checker/src/types/computation/access.rs` (~40 LOC, two new
  branches in `check_element_access_expression`).
- `crates/tsz-checker/src/types/computation/access_helpers.rs` (~50 LOC,
  new `concrete_receiver_write_target_should_preserve_indexed_access` +
  helper).
- `crates/tsz-checker/src/tests/noUIA_any_index_emits_ts2322_tests.rs`
  (new, ~150 LOC, 10 unit tests covering both fixes plus anti-hardcoding
  and negative-control coverage).
- `crates/tsz-checker/src/lib.rs` (1 line, register new test module).

## Verification

- `cargo test -p tsz-checker --lib` (3189 tests pass).
- `cargo test -p tsz-solver --lib` (5590 tests pass).
- `./scripts/conformance/conformance.sh run --filter "noUncheckedIndexedAccess"`
  (3/3 pass — the targeted test flips fingerprint-only → PASS, sister tests
  keep passing).
- `./scripts/conformance/conformance.sh run --filter "indexedAccess"` (13/13
  pass — no regressions in adjacent index-access conformance).
- `scripts/session/verify-all.sh --quick` (full unit + conformance regression
  gate).
