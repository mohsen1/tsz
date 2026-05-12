# fix(checker): preserve subclass instanceof checks after superclass exclusion

Status: ready
Branch: fix/instanceof-subclass-negative-narrowing-20260512
Issue: #6054

## Scope
- Fix negative `instanceof` narrowing so excluding a superclass does not collapse later subclass checks to `never` in tsc-accepted defensive code.
- Add a focused regression covering `Error` followed by `TypeError` after an early return.

## Validation plan
- Targeted checker regression test for the new case.
- Targeted existing instanceof narrowing tests if the changed path warrants it.

## Validation
- `cargo test -p tsz-checker --test conformance_issues test_instanceof_negative_superclass_keeps_subclass_check_usable`
- `cargo test -p tsz-checker --test conformance_issues test_instanceof_narrowing_with_class_hierarchy`
- `cargo fmt`
