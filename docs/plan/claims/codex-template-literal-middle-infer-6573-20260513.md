# codex/template-literal-middle-infer-6573-20260513

Status: ready
Owner: Codex
Created: 2026-05-13
Issue: #6573

## Scope

Fix template literal conditional matching for middle known substrings in patterns like `${infer Before}${Known}${infer After}`.

## Assumptions

- #6572 is adjacent but separate; this slice targets the middle-known-substring case from #6573.
- Solver ownership applies: implement matching in the solver, add focused regression coverage, avoid checker-side special cases.

## Validation log

- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests template_literal_middle_infer_matches_known_substring -- --nocapture` passed.
- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests -- --nocapture` passed: 14 passed.
- `cargo fmt --all --check` passed.
