# Fix readonly loss after `in` narrowing (#6332)

Status: claim
PR: TBD

## Scope

Investigate and fix missing TS2540 where readonly properties on union members become assignable after narrowing with an `in` check.

## Verification plan

- Reproduce #6332 with a focused CLI case.
- Add focused regression coverage for readonly preservation after `in` narrowing.
- Run the targeted test and direct CLI repro.
