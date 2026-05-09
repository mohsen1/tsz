# fix(checker): emit TS2416 when class implements predicate but body does not narrow

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-yNccB`
- **PR**: TBD
- **Status**: claim
- **Workstream**: TS2416 / class-implements parity (issue #4011)

## Intent

The class `implements` member-compatibility path was suppressing TS2416 for
*every* unannotated boolean-returning class method when the corresponding
interface method declared a parameter type predicate, regardless of whether
the class method's body actually had narrowing semantics tsc would infer
into a predicate. That hid real mismatches like
`isString(_v: string|number) { return true; }` against
`isString(value: string|number): value is string;`.

`signature_builder.rs` (and `function_type.rs`) already invoke
`try_infer_type_predicate_from_body` upstream and stamp the inferred
predicate onto the source signature when applicable. So the secondary
suppression in `query_boundaries/class.rs` was redundant in the
"inference succeeded" case and incorrect in the "inference correctly
returned None" case. Removing the suppression makes the relation logic
the single source of truth: when our inference matches tsc's
(`Acceptable` example), the predicates align and TS2416 is silent;
when the body cannot infer a predicate (`Broken` example), tsc reports
TS2416 and we now report it too.

## Files Touched

- `crates/tsz-checker/src/query_boundaries/class.rs` (removed
  `is_type_predicate_inference_suppressed` and
  `get_type_predicate_from_signature` helpers — ~100 LOC delete)
- `crates/tsz-checker/tests/class_implements_predicate_inference_tests.rs`
  (new — 4 unit tests covering inferable body, non-inferable body,
  alternate parameter name, explicit `: boolean` annotation)
- `crates/tsz-checker/Cargo.toml` (register new test target)

## Verification

- `cargo test -p tsz-checker --test class_implements_predicate_inference_tests`
  → 4 passed
- `cargo test -p tsz-checker --test mixin_base_no_member_no_ts2416_tests
  --test private_brands --test control_flow_type_guard_tests` → 79 passed
- `cargo test -p tsz-checker --lib` → 3734 passed; the 3 failures
  (`js_constructor_property_tests::checked_js_prototype_plain_parent_method_call_reports_ts2531`,
  `ts2300_tests::duplicate_identifier_with_default_lib_symbol_reports_lib_locations`,
  `ts2353_tests::recursive_array_union_excess_property_uses_outer_alias_display`)
  reproduce on plain `main` and are unrelated to this change.
- Manual repro from issue #4011 now correctly emits TS2416 for the
  `Broken` class while leaving `Acceptable` clean.
