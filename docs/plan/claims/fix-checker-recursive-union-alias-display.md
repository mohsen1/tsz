# fix(checker): expand recursive union alias in assignment diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-recursive-union-alias-display`
- **PR**: #3106
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only divergence in
`TypeScript/tests/cases/compiler/unionTypeWithRecursiveSubtypeReduction3.ts`.
The previous `fix/checker-typeof-typeliteral-no-circular` slice removed the
false `TS2456`; the current gap is the `TS2322` display text.

`tsc` expands the recursive `typeof` alias enough to show
`{ prop: number; } | { prop: { prop: number; } | ...; }`, while `tsz`
prints the alias name `T27`. This slice will align the assignment diagnostic
display without reintroducing circularity errors.

## Files Touched

- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs`
- `crates/tsz-checker/tests/type_alias_typeof_circular_tests.rs`

## Verification

- Baseline targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "unionTypeWithRecursiveSubtypeReduction3" --verbose`
  - Current result: fingerprint-only `TS2322`; expected display expands the
    recursive union and actual display is `T27`.
- Implementation checks:
  - `cargo fmt --check`
  - `CARGO_TARGET_DIR=/tmp/tsz-codex-conformance-next16-target cargo nextest run -p tsz-checker --lib ts2322_preserves_self_referencing_union_alias_name`
  - `CARGO_TARGET_DIR=/tmp/tsz-codex-conformance-next16-target cargo nextest run -p tsz-checker --test type_alias_typeof_circular_tests ts2322_expands_recursive_typeof_alias_source_display test_no_ts2456_when_typeof_target_references_alias_inside_type_literal`
  - `CARGO_TARGET_DIR=/tmp/tsz-codex-conformance-next16-target ./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "unionTypeWithRecursiveSubtypeReduction3" --verbose`
    - Result: `1/1 passed (100.0%)`
  - `CARGO_TARGET_DIR=/tmp/tsz-codex-conformance-next16-target ./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --max 200`
    - Result: `200/200 passed (100.0%)`

## Disk Notes

- Removed inactive worktree-local Rust artifacts during the session.
- Kept active Cargo targets intact while other worktrees were compiling.
- Used `/tmp/tsz-codex-conformance-next16-target` for final verification after
  the worktree-local `.target` was cleaned externally during a build; this
  temporary target was removed after verification.
