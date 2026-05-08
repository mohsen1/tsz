# checker: remove hardcoded Comparable<number> diagnostic rewrite

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-WZ9Fx`
- **PR**: #4577
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
  (retire test that relied on the rewrite; add a regression test for #3057)

## Verification

- `cargo nextest run -p tsz-checker --test generic_call_inference_tests -E 'test(/comparable|self_referential/)'`
  passes locally (4/4).
- `compiler/maxConstraints.ts` still passes at the error-code level
  (verified offline against `scripts/conformance/conformance-detail.json`
  pre-/post-removal).
- Full `cargo nextest run -p tsz-checker` was not completed in this
  session (local shell wedged before the run finished); CI on the PR
  will be the authoritative signal.
