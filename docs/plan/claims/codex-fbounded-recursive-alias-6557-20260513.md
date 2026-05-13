# Claim: fix F-bounded recursive alias corruption (#6557)

Status: ready
Owner: Codex
Branch: codex/fbounded-recursive-alias-6557-20260513
PR: #6558
Issue: #6557

## Scope
- Investigate and fix the TS2420 false positive where recursive interface + recursive alias context corrupts F-bounded `implements Comparable<MyNumber>` checking.
- Add focused regression coverage for the minimal repro.

## Validation
- `cargo test -p tsz-checker --test class_implements_index_signature_tests fbounded_implements_not_corrupted_by_recursive_alias_context -- --nocapture` passed.

## Notes
- The issue repro already passes on current main after the merged conformance/checker work; this PR adds focused regression coverage so it stays fixed.
