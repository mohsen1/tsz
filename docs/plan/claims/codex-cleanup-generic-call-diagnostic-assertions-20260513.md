# Claim: generic call inference diagnostic assertion cleanup

- Owner: Codex
- Date: 2026-05-13
- Issue: https://github.com/mohsen1/tsz/issues/6471
- Scope: Refactor repeated diagnostic filtering, counting, and message assertion boilerplate in `crates/tsz-checker/tests/generic_call_inference_tests.rs`.
- Coordination: DRY cleanup claim for issue/PR workflow; no behavior changes intended.
- Verification target: fmt, focused `generic_call_inference_tests`, checker clippy, CI unit package set, and full PR CI before merge.
