# fix: symbol index signature lookup accepts symbol keys

Status: ready
Issue: #6215
Branch: fix-symbol-index-signature-6215-20260513

## Scope
- Investigate and fix the TS7053/TS2322 false positive for indexing a symbol index signature with a symbol/unique symbol key.
- Add focused checker regression coverage for the reported reproduction.

## Coordination
- Created after checking open PRs/issues, active claims, remote branches, worktrees, local status, and disk space on 2026-05-13.
- Avoids open PR #6212 performance cache files and #6217 checker-test helper cleanup unless root cause requires nearby checker paths.

## Verification
- `cargo nextest run -p tsz-checker --lib -E 'test(symbol_index_signature_tests::annotated_symbol_index_signature_variable_allows_symbol_key_read)' --no-fail-fast` (1 passed)
- `cargo nextest run -p tsz-checker --lib -E 'test(symbol_index_signature_tests::)' --no-fail-fast` (8 passed)
- `cargo fmt --all -- --check`
- `git diff --check`

## Result
- Current `main` no longer reproduces #6215. This branch adds the reported reproduction as regression coverage so the symbol-index lookup behavior stays locked.
