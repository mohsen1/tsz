# chore(emitter/tests): expand IR builder helper coverage

- **Date**: 2026-04-26
- **Timestamp**: **2026-04-26 07:59:06**
- **Branch**: `chore/emitter-ir-builders-tests`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 8.4 (DRY emitter helpers / coverage)

## Intent

`crates/tsz-emitter/src/transforms/ir.rs` exposes ~25 small builder methods
(`IRNode::id`, `string`, `number`, `call`, `prop`, `elem`, `binary`, `assign`,
`var_decl`, `ret`, `func_expr`, `func_decl`, `this`, `this_captured`, `void_0`,
`paren`, `block`, `expr_stmt`, `object`, `empty_object`, `array`,
`empty_array`, `logical_or`, `logical_and`, `sequence`, plus `IRParam::new`,
`rest`, `with_default` and `IRProperty::init`).

`tests/ir_tests.rs` covers the printed-output side and ~half of the
construction helpers, but `elem`, `paren`, `object`/`empty_object`,
`array`/`empty_array`, `logical_or`, `logical_and`, `sequence`, the
`var_decl(_, None)` shape, and `IRProperty::init` have no direct unit tests
on the builder shape. This PR adds focused tests that lock in the variant /
field shape each builder produces, behaviour-preserving.

## Files Touched

- `crates/tsz-emitter/tests/ir_tests.rs` (~120 LOC additive)

## Verification

- `cargo nextest run -p tsz-emitter --lib` (full lib test suite passes)
