# fix(solver): preserve literal type when contextual type is an enum

- **Date**: 2026-04-28
- **Branch**: `fix/solver-contextual-literal-allow-enum-members`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance — enum literal assignment)

## Intent

`classify_for_contextual_literal` did not handle `TypeData::Enum`. For an assignment like `e = -1` where `e: E` and `enum E { A, B }`:

1. `get_type_of_prefix_unary` evaluates the contextual type `E` against the literal `-1`.
2. `contextual_type_allows_literal(E, -1)` calls `classify_for_contextual_literal(E)`, which falls through to `NotAllowed` (no `Enum` arm).
3. The unary returns the widened `number` type.
4. The subtype check `number → E` succeeds via the open-numeric-enum rule (TypeScript-equivalent unsoundness item #7), so no TS2322 fires.

tsc keeps the source `-1` literal because it recognises the enum context as a union of member values; the structural subtype check then rejects `-1` against `0 | 1` and emits TS2322.

Fix: add a `TypeData::Enum(_, members)` arm that returns `Members(vec![members])` so the recursive classifier walks into the enum's member-value union. The argument keeps its literal type, the structural subtype check rejects out-of-range literals, and TS2322 fires at the assignment site.

Surfaced by `conformance/types/primitives/enum/validEnumAssignments.ts` (missing TS2322 on `e = -1;`). Also unblocks `enum_nominality_tests::test_number_literal_to_numeric_enum_type`, which had been `#[ignore]`d with the comment _"unit checker does not load lib types needed for this enum case"_ — the issue was unrelated to lib loading; it was the same missing classifier arm.

## Files Touched

- `crates/tsz-solver/src/type_queries/extended.rs` — add `TypeData::Enum` arm in `classify_for_contextual_literal`.
- `crates/tsz-checker/tests/enum_nominality_tests.rs` — un-ignore `test_number_literal_to_numeric_enum_type`; add `test_negative_number_literal_to_numeric_enum_type` that locks the new behavior.

## Verification

- `cargo nextest run -p tsz-solver --lib` — 5543/5543 pass.
- `cargo nextest run -p tsz-checker --test enum_nominality_tests` — 21/21 pass (was 20/21 with one ignored).
- `./scripts/conformance/conformance.sh run --filter "validEnumAssignments"` — 2/2 PASS (was 1/2 fingerprint-only).
- `./scripts/conformance/conformance.sh run --filter "Enum"` — 178/180 PASS (the 2 failures are pre-existing on main, unrelated to this change).
