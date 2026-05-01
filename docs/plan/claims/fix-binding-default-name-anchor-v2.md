**2026-04-26 19:30:00**

# fix(checker): anchor binding-default TS2322 on the binding name

Status: shipped (PR #1433 — `fix(checker): anchor binding-default TS2322 on the binding name`, merged 2026-04-26)
Branch: fix/binding-default-name-anchor-v2

Goal: align tsc fingerprint for `function h({ prop = "baz" }: StringUnion)`-style
destructuring defaults so TS2322 emits at the binding name (`prop`) rather than
the initializer expression (`"baz"`). Source-side elaboration paths (arrow-body
return, object/array literals) keep their own anchors.

Test target:
- `TypeScript/tests/cases/conformance/types/contextualTypes/methodDeclarations/contextuallyTypedBindingInitializerNegative.ts`
  (was fingerprint-only, now flips one extra+one missing fingerprint to match).

Scope: thin checker change in `check_binding_element_with_request` to switch
from `check_assignable_or_report` to `check_assignable_or_report_at` with
`diag_idx = element_data.name`. Solver and boundary helpers untouched.

Conformance impact: net +3 tests (12183 -> 12186), 0 regressions.
