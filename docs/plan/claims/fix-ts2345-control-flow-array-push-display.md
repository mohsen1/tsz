# fix(checker): align control-flow array push TS2345 fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix-ts2345-control-flow-array-push-display`
- **PR**: #2799
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the refreshed random conformance pick `controlFlowArrayErrors.ts`, where
`tsc` and `tsz` emit the same TS2345/TS7005/TS7034 codes but disagree on two
TS2345 fingerprints for evolving-array `push` calls. The expected TS2345
surfaces are `99` against `never` and `"hello"` against `number`.

## Files Touched

- `docs/plan/claims/fix-ts2345-control-flow-array-push-display.md`
- `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`
- `crates/tsz-checker/src/flow/control_flow/core.rs`
- `crates/tsz-checker/src/flow/flow_graph_builder/expressions.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call/mod.rs`
- `crates/tsz-checker/tests/conformance_issues/features/implicit_any.rs`

## Verification

- `PATH="$HOME/.cargo/bin:$PATH" cargo check -p tsz-checker`
- `PATH="$HOME/.cargo/bin:$PATH" cargo nextest run -p tsz-checker --test conformance_issues test_evolving_array_push_snapshots_and_branch_merges_match_tsc`
- `PATH="$HOME/.cargo/bin:$PATH" cargo nextest run -p tsz-checker --test conformance_issues features::implicit_any`
- `./scripts/conformance/conformance.sh run --filter "controlFlowArrayErrors" --verbose` — 1/1 passed.
- `./scripts/conformance/conformance.sh run --filter "controlFlowArrays" --verbose` — 1/1 passed.
- `scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh run` — 12450/12582 passed, net +13 versus baseline. Remaining PASS -> FAIL deltas were `dynamicNames.ts`, `noImplicitAnyIndexing.ts`, and `noUncheckedIndexedAccess.ts`; `dynamicNames.ts` was reproduced after reverting this patch, and the two indexing deltas are outside the touched evolving-array path.
