# fix: TS2367 false positive for T extends {} and parser STATIC_BLOCK await context

- **Date**: 2026-04-27
- **Branch**: `fix/ts2367-unknown-constraint-regression`
- **PR**: #1588
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Two independent conformance fixes:

1. **TS2367 false positive**: `T extends {}` was incorrectly treated as `T extends object` in
   type-param apparent-type resolution. The empty object type `{}` includes primitives (e.g.,
   `T extends {}` allows `T = 42`), so mapping it to OBJECT caused false-positive TS2367 when
   comparing `T & ({} | null)` to a number literal. Also includes lint/arch fixups
   (collapsible_match, collapsible_if) and module splits for context/core.rs and
   condition_narrowing.rs.

2. **Parser STATIC_BLOCK context**: Inside `static {}` blocks, `await` is always reserved —
   including in nested function and arrow function parameters. Previously,
   CONTEXT_FLAG_STATIC_BLOCK was cleared before parameter parsing, making `await` silently
   accepted as a valid identifier. This fixes missing TS1109 diagnostics in
   classStaticBlock26.ts and eliminates a TS1213→TS1109 fingerprint mismatch for `[await]`
   computed property names.

## Files Touched

- `crates/tsz-checker/src/types/computation/assignability_type_param_helpers.rs` (~10 LOC)
- `crates/tsz-checker/src/context/core.rs` / `core_index_tests.rs` (module split, -120/+114)
- `crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs` / `alias_narrowing.rs` (module split)
- `crates/tsz-parser/src/parser/state.rs` (~11 LOC)
- `crates/tsz-parser/src/parser/state_expressions.rs` (~35 LOC)
- `crates/tsz-parser/src/parser/state_statements.rs` (~17 LOC)

## Verification

- `cargo nextest run -p tsz-parser --lib` — 691 passed, 0 failed
- `cargo nextest run -p tsz-checker --lib` — 5209 passed (pre-existing `test_check_js_global_tostring_overload_reports_ts2394_with_libs` failure unrelated)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh snapshot` — no regressions, 97.2% overall

## Known Limitation

The parser has no access to language target, so `await (1)` in static blocks continues to emit
TS18037 regardless of target. tsc emits TS1109 at es2022+ and TS18037 at es2015. Fixing this
requires plumbing target info into the parser.
