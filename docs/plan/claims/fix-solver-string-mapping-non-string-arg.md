# fix(solver): accept literals against StringMapping over non-string primitives

- **Date**: 2026-04-26
- **Branch**: `fix/solver-string-mapping-non-string-arg`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the false-positive TS2322 emitted by tsz when assigning a string
literal to a string-mapping wrapper applied to a non-string primitive
pattern, e.g.:

```ts
declare var y: Uppercase<`${number}`>;
y = "1";  // tsc accepts; tsz incorrectly errored
```

This affected the `stringLiteralsAssignedToStringMappings.ts` conformance
target and any similar pattern using `Uppercase<\`${number}\`>`,
`Lowercase<\`${number}\`>`, or nested compositions over `${bigint}` /
`${boolean}`.

## Root cause

Two compounding bugs in the solver:

1. **`evaluate_string_intrinsic` returned `TypeId::ERROR`** when the type
   argument was a non-string primitive intrinsic (`number`, `bigint`,
   `boolean`). After tsz's eager normalization
   `Uppercase<\`${number}\`>` → `\`${Uppercase<number>}\``, the inner
   `Uppercase<number>` evaluated to ERROR, derailing downstream pattern
   matching.
2. **`visit_literal` only accepted `Mapping<string>` targets** (the
   fixed-point check `Mapping(literal) == literal` was gated on
   `type_arg == TypeId::STRING`). Even after fixing (1), the rule didn't
   apply when the target was `Uppercase<number>`.

## Fix

- `crates/tsz-solver/src/evaluation/evaluate_rules/string_intrinsic.rs`:
  preserve `Mapping<T>` for `T ∈ {number, bigint, boolean}` instead of
  collapsing to ERROR. Mirrors tsc's preservation of pattern-literal
  placeholder types via `getStringMappingType` + `isPatternLiteralType`.
- `crates/tsz-solver/src/relations/subtype/visitor.rs`:
  generalize the literal-to-StringMapping rule. A string literal `s` is
  assignable to `Mapping<T>` iff (a) `Mapping(s_value) == s_value` AND
  (b) `s` matches the stringification template `\`${T}\`` (computed by
  the existing template-literal pattern matcher).

## Files Touched

- `crates/tsz-solver/src/evaluation/evaluate_rules/string_intrinsic.rs`
  (+11 LOC: preserve StringMapping for non-string primitive args)
- `crates/tsz-solver/src/relations/subtype/visitor.rs`
  (+27 LOC, -3 LOC: generalize the literal-vs-mapping fixed-point rule)
- `crates/tsz-solver/tests/string_intrinsic_subtype_tests.rs`
  (+122 LOC: 5 new unit tests pinning the new behaviour and rejection
   semantics)

## Verification

- `cargo nextest run -p tsz-solver` — 5514 PASS, 0 FAIL.
- `cargo nextest run -p tsz-solver -E 'test(uppercase_over_number) or
  test(lowercase_over_number) or
  test(nested_uppercase_lowercase_over_number) or
  test(evaluate_uppercase_over_number_intrinsic_is_preserved)'` — 5/5 PASS.
- Targeted conformance: `stringLiteralsAssignedToStringMappings.ts`
  drops from 5 emitted errors → 3 (matches tsc's count); now blocked
  only on type-display parity and line-offset bugs orthogonal to this
  fix.
- Regression sample: `--filter String` 325/336, `--filter template`
  197/205 — no regressions vs main.
