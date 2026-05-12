# fix(checker): preserve subclass instanceof checks after superclass exclusion

Status: claim
Branch: fix/instanceof-subclass-negative-narrowing-20260512
Issue: #6054

## Scope
- Fix negative `instanceof` narrowing so excluding a superclass does not collapse later subclass checks to `never` in tsc-accepted defensive code.
- Add a focused regression covering `Error` followed by `TypeError` after an early return.

## Validation plan
- Targeted checker regression test for the new case.
- Targeted existing instanceof narrowing tests if the changed path warrants it.
