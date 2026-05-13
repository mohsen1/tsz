# fix: imported Generator remains iterable across files

Status: ready
Issue: #6213
Branch: fix-imported-generator-iterable-6213-20260513

## Scope
- Investigate and fix the TS2488 false positive for `for-of` over an imported function returning `Generator<T>`.
- Add focused multi-file checker regression coverage for the issue reproduction.

## Coordination
- Created after checking open PRs/issues, active claims, remote branches, worktrees, local status, and current main on 2026-05-13.
- Avoids #6212 performance-cache files, #6217 checker-test cleanup, and #6218 symbol-index regression files unless root cause requires shared infrastructure.

## Verification
- `cargo nextest run -p tsz-checker --lib -E 'test(imported_generator_iterable_tests::imported_generator_return_type_is_iterable_in_for_of)' --no-fail-fast` (1 passed)
- `cargo fmt --all`
- `git diff --check`

## Result
- Current `main` no longer reproduces #6213 in a focused multi-file harness. This branch adds regression coverage for the imported `Generator<T>` for-of path.
