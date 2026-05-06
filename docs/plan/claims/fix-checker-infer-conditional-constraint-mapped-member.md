# Claim: infer conditional constraint mapped member

Branch: `codex/infer-conditional-constraint-mapped-member-20260506`

Target conformance case:

- `TypeScript/tests/cases/compiler/inferConditionalConstraintMappedMember.ts`

Current snapshot classification:

- Category: false-positive
- Expected diagnostics: none
- Actual diagnostics: `TS2344`
- Extra diagnostics: `TS2344`

Planned scope:

- Investigate why `KeysWithoutStringIndex<T>` is treated as `unknown` for the
  `Pick<T, ...>` key constraint.
- Narrowly suppress or improve the constraint relation so mapped-key filtering
  remains a subset of `keyof T` without changing unrelated generic constraint
  checking.
- Add focused checker coverage and verify the filtered conformance case.

Verification to fill before ready:

- `cargo fmt --check`
- focused checker test(s)
- filtered conformance for `inferConditionalConstraintMappedMember`
