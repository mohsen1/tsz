# fix(checker): skip type resolution in duplicate `extends` clauses on classes

- **Date**: 2026-05-03
- **Branch**: `fix/parser-skip-duplicate-extends-clause-types`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance — parser recovery cascades

## Intent

`class C extends A extends B {}` is a TS1172 parser error (`'extends' clause
already seen`). tsc reports TS2304 only for `A` (the first extends operand)
and the parser-level TS1172. tsz additionally tried to resolve `B`, which
emitted a spurious TS2304 ("Cannot find name 'B'") on top of the parser
error -- the conformance fingerprint mismatch on
`parserClassDeclaration1.ts`.

Track `extends_seen` in three heritage walkers (the unresolved-name walker,
the base-instance-type walker, and the heritage-expression-type walker) and
skip subsequent extends clauses for class declarations. Interfaces still
walk every extends clause (they may legitimately extend multiple types).

## Files Touched

- `crates/tsz-checker/src/state/state_checking/heritage.rs` (~10 LOC)
- `crates/tsz-checker/src/state/state_checking/class.rs` (~14 LOC across two walkers)
- `crates/tsz-checker/src/tests/class_duplicate_extends_skip_resolution_tests.rs` (new, ~36 LOC)
- `crates/tsz-checker/src/lib.rs` (+3 LOC: register the new test module)

## Verification

- `cargo nextest run -p tsz-checker --lib` passes
- `./scripts/conformance/conformance.sh run --filter "parserClassDeclaration1"` -- 11 / 11 passing
- Full conformance: net +1 (`parserClassDeclaration1.ts` flips FAIL -> PASS), no regressions
