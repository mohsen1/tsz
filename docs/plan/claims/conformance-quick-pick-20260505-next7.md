# [WIP] fix(checker): align JS element access contextual type diagnostic

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next10`
- **PR**: #2883 (follow-up to #2825)
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance fingerprint mismatch in
`jsElementAccessNoContextualTypeCrash.ts`. The picked failure has matching
diagnostic codes, but tsz reports TS2741 at `self['Common'] || {}` with type
display `Common`; tsc reports the diagnostic at the statement start with
display `typeof Common`.

## Files Touched

- `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`
- `crates/tsz-checker/src/assignability/assignment_checker/js_global_fallback.rs`
- `crates/tsz-checker/src/assignability/assignment_checker/mod.rs`
- `crates/tsz-checker/tests/conformance_issues/features/async.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/compiler/jsElementAccessNoContextualTypeCrash.ts`.
- `cargo nextest run -p tsz-checker --test conformance_issues test_js_global_element_access_or_fallback_uses_contextual_target`
  passed.
- `./scripts/conformance/conformance.sh run --filter "jsElementAccessNoContextualTypeCrash" --verbose`
  passed 1/1 on the rebased branch.
- `cargo fmt --check` passed.
- `git diff --check` passed.
- `cargo nextest run -p tsz-solver typeof_prefix_for_namespace_and_class_constructor_defs`
  passed.
- `cargo nextest run -p tsz-checker architecture_contract_tests_src::test_checker_file_size_ceiling`
  passed.
- Repo pre-commit hook passed while creating the implementation commit
  (`14976 passed, 55 skipped` in affected-crate nextest).
- Full conformance on the rebased branch:
  `CARGO_BUILD_JOBS=4 ./scripts/conformance/conformance.sh run`
  reported `12453/12582 passed (99.0%)`, `Fingerprint-only: 84`, net
  `12451 -> 12453 (+2)`, including
  `jsElementAccessNoContextualTypeCrash.ts` as an improvement. The reported
  `nestedRecursiveArraysOrObjectsError01.ts` PASS -> FAIL delta was also
  reproduced on a clean `origin/main` worktree, so it is baseline drift, not
  this checked-JS-only PR.
