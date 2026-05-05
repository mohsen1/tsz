# fix(checker): preserve generic function inference contexts

- **Date**: 2026-05-05
- **Branch**: `fix/checker-generic-function-inference1`
- **PR**: https://github.com/mohsen1/tsz/pull/3095
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Fix the extra diagnostics in
`TypeScript/tests/cases/compiler/genericFunctionInference1.ts`.
The picked fixture should only report `TS2345`, but `tsz` also emitted
`TS2322` and `TS2362`.

The fix keeps generic function arguments from being over-instantiated by
rest-tuple contextual signatures while still refining fixed-parameter
contexts that carry outer type parameters. It also filters speculative
callback-body diagnostics once overload resolution succeeds, preventing
discarded contextual passes from leaking arithmetic errors.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
  - Refines generic function arguments only against fixed-parameter
    contextual signatures whose return type depends on type parameters.
- `crates/tsz-checker/src/types/computation/call_inference.rs`
  - Leaves source generic function arguments unchanged for rest-parameter
    target signatures.
- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
  - Prunes callback-body speculative diagnostics after successful overload
    resolution while preserving non-callback candidate diagnostics.
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
  - Adds regressions for chained `pipe` callback return contexts and
    curried `map(identity)` array element inference.
- `docs/plan/claims/fix-checker-generic-function-inference1.md`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test generic_call_inference_tests overloaded_pipe_return_context_types_chained_callback_params curried_map_identity_preserves_array_element_type`
  - `2 tests passed`
- `cargo build --target-dir .target --profile dist-fast -p tsz-cli -p tsz-conformance`
- `./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --filter genericFunctionInference1 --verbose --print-fingerprints --workers 1 --no-batch --tsz-binary ./.target/dist-fast/tsz`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
- Disk cleanup:
  - Removed stale Rust `.target` / `target` directories from the repo and
    worktrees before rebuilding.
  - Current worktree Rust target artifacts: none.
  - Current filesystem state: `243Gi` available, `46%` used.
  - Left active Rust artifacts in other worktrees intact while those cargo
    processes were still running.
