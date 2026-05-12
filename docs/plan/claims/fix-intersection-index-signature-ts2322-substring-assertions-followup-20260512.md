# Claim: intersection index-signature TS2322 assertion hardening

Status: ready
Owner: Codex
Branch: codex/review-audit-followup-20260512
Source PR/comment: #5114 follow-up (missed-review audit)

## Target

Reduce brittle full-message equality checks in intersection index-signature TS2322 tests while preserving the intended semantic coverage.

## Plan

1. Replace exact-string TS2322 message assertions with key substring checks.
2. Keep strict checks for both source intersection surface and target index-signature surface.

## Result

- Updated tests:
  - `assignment_to_index_signature_preserves_declared_intersection_and_alias_surfaces`
  - `assignment_to_primitive_index_signature_preserves_anonymous_intersection_surface`
- File:
  - `crates/tsz-checker/tests/intersection_index_signature_fingerprint_tests.rs`

Validation:

```text
cargo fmt --all
cargo test -p tsz-checker --test intersection_index_signature_fingerprint_tests assignment_to_ -- --nocapture
```
