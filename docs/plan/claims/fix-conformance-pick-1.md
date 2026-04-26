# fix(checker): elaborate array-literal element error when generic param resolves to concrete

- **Date**: 2026-04-26
- **Branch**: `fix/conformance-pick-1`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance fingerprint parity

## Intent

When a call argument is an array literal targeting a generic parameter
(e.g. `f<T extends string[], U extends string>(arg: { [K in U]: T }[U])`),
both array-literal elaboration paths bailed out early via
`call_argument_targets_generic_parameter`. tsc, however, still emits a
TS2322 element-level error pointing at the offending element when
inference falls back to the constraint and the resolved source/target
types are concrete (e.g., `number[]` vs `string[]`).

This change keeps the heuristic but only short-circuits when the
*resolved* source/target types still contain unresolved type parameters
or infer placeholders. When both sides are concrete, elaboration
proceeds as it would for a non-generic call, matching tsc.

Flips `inferenceShouldFailOnEvolvingArrays.ts` from FAIL to PASS.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs` (+15 LOC)
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration_array_mismatch.rs` (+19 LOC)
- `crates/tsz-checker/tests/ts2322_tests.rs` (+50 LOC regression test)

## Verification

- `cargo nextest run -p tsz-checker --test ts2322_tests -E 'test(ts2322_array_element_elaborated_when_generic_param_resolves_to_concrete_constraint)'` (1 test passes)
- `./scripts/conformance/conformance.sh run --filter "inferenceShouldFailOnEvolvingArrays"` (1/1 PASS)
- `./scripts/conformance/conformance.sh run --filter "arrayLiteral"` (19/19 PASS)
- `./scripts/conformance/conformance.sh run --filter "indexedAccess"` (13/13 PASS)
- `./scripts/conformance/conformance.sh run --filter "elaboration"` (1/1 PASS)
- `./scripts/conformance/conformance.sh run --filter "tuple"` (30/37 — same as baseline)
- `./scripts/conformance/conformance.sh run --filter "mappedType"` (45/55 — same as baseline)
- `./scripts/conformance/conformance.sh run --filter "callWith"` (9/9 PASS)
