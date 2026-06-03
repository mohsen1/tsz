//! Architecture guard: type-parameter constraint violations are reported only
//! through the shared assignability gateway, never through a dedicated
//! `CallResult` bypass variant.
//!
//! Regression guard for #10901 ("TS2322 family path bypasses query boundaries
//! in nested conditional calls"). The orphaned
//! `CallResult::TypeParameterConstraintViolation` arm in
//! `types/computation/call_result.rs` emitted TS2345 through the raw
//! `error_argument_not_assignable_at` emitter, bypassing
//! `query_boundaries::assignability`, while the equivalent `new`-expression arm
//! in `types/computation/complex.rs` routed through the gateway. That asymmetry
//! is exactly the "one path emits false errors while another returns success"
//! symptom the issue describes.
//!
//! Constraint violations now flow solely through
//! `CallResult::ArgumentTypeMismatch`, whose checker dispatch routes through
//! `check_argument_assignable_or_report` (relation -> reason -> diagnostic).
//! These guards keep the bypass variant from being reintroduced on either the
//! solver (where the variant lived) or the checker (where it was matched).

use std::fs;

/// The solver must not reintroduce a dedicated constraint-violation `CallResult`
/// variant. Constraint violations are surfaced as `ArgumentTypeMismatch` so the
/// checker re-checks them through the assignability gateway (which evaluates
/// conditional/generic constraints in the proper environment) instead of
/// emitting unconditionally.
///
/// The solver crate sets `autotests = false`, so this cross-crate invariant is
/// asserted here, alongside the checker dispatch guard it pairs with.
#[test]
fn no_type_parameter_constraint_violation_callresult_variant() {
    let solver_src = fs::read_to_string("../tsz-solver/src/operations/core/call_evaluator.rs")
        .expect("failed to read solver call_evaluator.rs for architecture guard");
    assert!(
        !solver_src.contains("TypeParameterConstraintViolation"),
        "the orphaned `CallResult::TypeParameterConstraintViolation` bypass variant must \
         not be reintroduced; constraint violations route through \
         `CallResult::ArgumentTypeMismatch` and the assignability gateway"
    );
}

/// Neither call-expression dispatch site may match a dedicated
/// constraint-violation arm and emit through the raw argument emitter.
#[test]
fn call_dispatch_has_no_constraint_violation_bypass_arm() {
    for path in [
        "src/types/computation/call_result.rs",
        "src/types/computation/complex.rs",
    ] {
        let src = fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("failed to read {path} for architecture guard: {err}"));
        assert!(
            !src.contains("TypeParameterConstraintViolation"),
            "{path} must not match a `TypeParameterConstraintViolation` arm; constraint \
             violations are handled through `CallResult::ArgumentTypeMismatch`, which routes \
             through `check_argument_assignable_or_report`"
        );
    }
}
