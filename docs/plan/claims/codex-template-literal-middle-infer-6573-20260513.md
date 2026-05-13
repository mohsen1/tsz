# codex/template-literal-middle-infer-6573-20260513

Status: ready
Owner: Codex
Created: 2026-05-13
Issues: #6572, #6573, #6580

## Scope

Fix template literal conditional matching for middle known substrings in patterns like `${infer Before}${Known}${infer After}`. Add focused coverage for adjacent trailing-suffix and type-parameter-delimiter patterns that are covered by the same matcher change.

## Assumptions

- The same solver matcher change covers #6572 and #6580, so the PR records regressions for those reports too instead of splitting identical implementation surface.
- Solver ownership applies: implement matching in the solver, add focused regression coverage, avoid checker-side special cases.

## Validation log

- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests template_literal_middle_infer_matches_known_substring -- --nocapture` passed.
- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests -- --nocapture` passed: 14 passed.
- `cargo fmt --all --check` passed.
- `cargo test -p tsz-checker --test infer_extends_constraint_substitution_tests -- --nocapture` passed after adding #6572/#6580 regressions: 16 passed.
