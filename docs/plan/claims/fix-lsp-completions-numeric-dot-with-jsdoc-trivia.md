# fix(lsp): suppress completions after `0./** comment */`-style numeric-dot-with-trailing-trivia

- **Date**: 2026-04-28
- **Branch**: `fix/lsp-completions-numeric-dot-with-jsdoc-trivia`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints) — fourslash subset

## Intent

`completionListAfterNumericLiteral.ts` (fourslash) failed at marker `dotOnNumberExpressions4` for input `0./** comment */<cursor>`. tsc's completion provider operates at the *token* level: the previous token is `0.` (a complete decimal NumericLiteral), so completions are suppressed (no member access).

tsz's `is_ambiguous_numeric_dot_context` (text-based suppression) walked back from the cursor through whitespace but not through block comments. For `0./** comment */<cursor>`, the line ended with `*/` and the suffix-`'.'`-check returned false — completions leaked through.

Fix: strip a single trailing block comment before the suffix check. After this, `0./** comment */` reduces to `0.`, the suffix-`'.'`-check matches, and completions are correctly suppressed.

Flips one fourslash test from FAIL to PASS: `completionListAfterNumericLiteral`.

## Files Touched

- `crates/tsz-lsp/src/completions/filters.rs` — strip a single trailing `/*…*/` from `prefix` before the existing suffix-`'.'` check (~9 LOC).
- `crates/tsz-lsp/tests/completions_tests.rs` — `test_completions_suppressed_after_numeric_dot_with_jsdoc_trivia` locks the behavior.

## Verification

- `cargo nextest run -p tsz-lsp` (3733 tests pass, 1 new lock-in test)
- `scripts/fourslash/run-fourslash.sh --filter=completion --workers=8` → 822/822 (was 821/822)
- `scripts/fourslash/run-fourslash.sh --filter=completionListAfterNumericLiteral` → 2/2 (was 1/2)
