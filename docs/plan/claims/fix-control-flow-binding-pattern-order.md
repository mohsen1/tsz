# fix: prevent flow narrowing of const destructured bindings through computed property dependencies

- **Date**: 2026-05-01
- **Branch**: `fix/control-flow-binding-pattern-order`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance (controlFlowBindingPatternOrder)

## Intent

Fixes a fingerprint-only conformance failure where const destructured bindings
with computed property keys were incorrectly narrowed by flow analysis.
The `check_flow` ASSIGNMENT handler was recomputing the destructuring assigned
type using already-narrowed types of dependent variables (e.g., computed key
variables), discarding union members introduced by default-initializer widening.
Additionally fixes `is_const_symbol` to walk up the parent chain from
BINDING_ELEMENT to the enclosing VARIABLE_DECLARATION, so const-ness is
correctly detected for destructured bindings.

## Files Touched

- `crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs` (~30 LOC)
- `crates/tsz-checker/src/flow/control_flow/core.rs` (~30 LOC)
- `crates/tsz-checker/tests/ts2322_tests.rs` (~40 LOC)

## Verification

- `cargo nextest run -p tsz-checker -p tsz-solver` (all pass)
- `./scripts/conformance/conformance.sh run --filter controlFlowBindingPatternOrder` (1/1 pass)
