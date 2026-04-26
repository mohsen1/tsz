# fix(checker/assignability): unsuppress TS2322 for `T -> \`${T}\`` patterns

- **Date**: 2026-04-26
- **Branch**: `fix/checker-template-literal-suppression-typeparam-source`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

Restore `tsc` parity for assigning a bare type parameter to a template
literal pattern that references the same type parameter (for example
`const test1: \`${T}\` = x;` inside `function f<T extends "a"|"b">(x: T)`).
The existing `should_suppress_assignability_diagnostic` "complex type"
heuristic was silently swallowing TS2322 here because `\`${T}\`` "contains"
T but is not itself a type parameter, so the generic
`contains_type_parameters && !type_parameter_like` suppression fired. The
fix narrows that suppression with a template-literal-target / bare
type-parameter-source carve-out, so the actual assignability query runs
and the canonical solver diagnostic surfaces. The carve-out stays narrow
on purpose: template-vs-template generic assignments that `tsc` tolerates
(e.g. `\`...${Uppercase<T>}.4\``  vs `\`...${Uppercase<T>}.3\``) keep
their existing suppression.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_checker.rs`
  (~16 LOC: extend the suppression call site with a template-literal
  target / type-parameter source carve-out)
- `crates/tsz-checker/src/assignability/assignment_checker_tests.rs`
  (~55 LOC: two regression tests — one locks in the new TS2322
  emission, one locks in the still-suppressed template-vs-template path)

## Verification

- `cargo nextest run -p tsz-checker --lib` (2888 pass)
- `cargo nextest run -p tsz-solver --lib` (5514 pass)
- `./scripts/conformance/conformance.sh run --filter "templateLiteral"`
  (templateLiteralTypes5 now PASS; templateLiteralTypes3 stays PASS;
  no other template-literal regressions)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`:
  net 12183 -> 12184 (+1), one improvement
  (`templateLiteralTypes5.ts`), zero new failures.
