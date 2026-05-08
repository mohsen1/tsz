# checker: remove hardcoded Comparable<number> diagnostic rewrite

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-WZ9Fx`
- **PR**: TBD
- **Status**: claim
- **Workstream**: anti-hardcoding (CLAUDE.md §25)

## Intent

Issue #3057: the checker post-processes diagnostics with
`rewrite_numeric_literal_generic_call_fingerprints`, which scans the call-site
source text and rewrites any `Comparable<number>` it finds in a TS2345 message
to a literal-union form like `Comparable<1 | 2>`. This corrupts user-facing
diagnostics for any real `Comparable<number>` parameter declared by user code,
and is a textbook example of the §25 anti-hardcoding directive: it keys on a
literal type name and on numeric tokens scraped from source text. Remove the
rewrite and update the existing self-referential test to assert the actual
solver-produced display.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
  (delete `rewrite_numeric_literal_generic_call_fingerprints`)
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
  (update test that relied on the rewrite; add a regression test for #3057)

## Verification

- `cargo nextest run -p tsz-checker --test generic_call_inference_tests`
  (locally, the two relevant tests + the existing siblings pass)
- Underlying TS2345 error code remains correct on `compiler/maxConstraints.ts`
  (passes at error-code level in conformance pre-/post-removal)
