# fix(checker): never variable initializers no longer suppress TS2355

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-joCe5`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / checker flow analysis

## Intent

Issue #3662: `function f(): number { const value = fail(); }` (where `fail`
returns `never`) currently exits cleanly under tsz, but tsc reports TS2355.
The reachability checker treats a variable declaration whose initializer is
a never-typed call as a terminating statement, but tsc's flow graph only
marks bare expression-statement calls (e.g. `fail();`) as unreachable —
variable declarations always fall through. This PR drops the
`VARIABLE_STATEMENT` arm in `statement_falls_through` so variable
declarations follow the default fall-through path.

## Files Touched

- `crates/tsz-checker/src/flow/reachability_checker.rs` — drop the
  `VARIABLE_STATEMENT` special case so variable declarations always fall
  through, matching tsc's `bindCallExpressionFlow` (which only flags bare
  call expressions as terminating).
- `crates/tsz-checker/tests/never_returning_narrowing_tests.rs` — add
  regression tests covering `const`, `let`, declarator-list, and the
  expression-statement control case, with two name choices to keep the
  rule structural per §25.
- `crates/tsz-core/tests/checker_state_tests.rs` — update the existing
  `test_never_returning_call_no_2355` test that previously asserted
  `usesFailInInit` / `usesFailInList` should not trigger TS2355 (they
  encoded the buggy behaviour).

## Verification

- `cargo test -p tsz-checker --lib never_returning` — all 14 narrowing
  tests pass, including the four new ones.
- `cargo test -p tsz-core --lib` — full lib suite green (3138 passed).
- `cargo test -p tsz-solver --lib` — full lib suite green (5698 passed).
- `cargo test -p tsz-checker --lib` — pre-existing failures unrelated to
  this change (verified by stashing the diff and re-running):
  `js_constructor_property_tests::checked_js_prototype_plain_parent_method_call_reports_ts2531`,
  `ts2300_tests::duplicate_identifier_with_default_lib_symbol_reports_lib_locations`,
  `ts2353_tests::recursive_array_union_excess_property_uses_outer_alias_display`.
- Manual repro from #3662: `tsz --noEmit --strict /tmp/repro_3662.ts` now
  emits `TS2355: A function whose declared type is neither 'undefined',
  'void', nor 'any' must return a value.` at the expected location.
