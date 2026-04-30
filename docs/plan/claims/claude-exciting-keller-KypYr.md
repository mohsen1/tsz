# conformance: fix reverse-mapped tuple inference for homomorphic Application contexts

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-KypYr`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Fix `reverseMappedTupleContext.ts` (GitHub #55382): array literals passed to
functions whose parameter is a homomorphic mapped type alias application (e.g.,
`Definition<Schema>`) were not inferred as tuples, causing a false-positive
TS2345. The fix has two components:

1. **Case 6b in `reverse_infer_through_template`**: When the source value is a
   Tuple and the template is a Mapped type, reverse-infer each tuple element
   against its position-specific template instance. This handles
   `Mapped<Tuple>` the same way Case 6a handles `Mapped<Object>`.

2. **`force_tuple_for_mapped` in `array_literal.rs`**: When the contextual
   type is an Application (`Definition<Schema>`) rather than a bare Mapped
   type, the existing `is_homomorphic_mapped_type_context` check incorrectly
   returns false (the Application gets expanded to an Object by
   `evaluate_contextual_type`). The fix adds
   `original_context_is_homomorphic_mapped_application` which looks at the
   generic definition body directly, recognizing that `Definition<T>` has body
   `{ [K in keyof T]: ... }` — a homomorphic Mapped type.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/reverse_mapped.rs` — Case 6b Tuple handling
- `crates/tsz-checker/src/types/computation/array_literal.rs` — `force_tuple_for_mapped` + helper
- `crates/tsz-checker/tests/reverse_mapped_inference_tests.rs` — 3 new unit tests

## Verification

- `reverseMappedTupleContext.ts` conformance test: 1/1 passed (was 0/1)
- `cargo check -p tsz-checker -p tsz-solver`: clean
- Net conformance delta: +1 test
