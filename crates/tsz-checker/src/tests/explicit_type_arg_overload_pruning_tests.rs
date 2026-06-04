//! Regression tests for explicit type-argument overload pruning.
//!
//! Structural rule: when a call supplies explicit type arguments, overloads
//! whose type-parameter constraints cannot accept those arguments are not viable
//! candidates for the instantiated call signature set. Ambiguous or diagnostic
//! cases still fall back to the full overload family.

use crate::test_utils::{check_source_diagnostics, diagnostic_codes};

#[test]
fn explicit_type_arg_constraint_prunes_wrongly_ordered_overload() {
    let diagnostics = check_source_diagnostics(
        r#"
declare function choose<TValue extends number>(value: unknown): "number";
declare function choose<TItem extends string>(value: unknown): "string";

const selected: "string" = choose<"literal">(0);
"#,
    );

    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.contains(&2322),
        "explicit string type argument should prune the number-constrained overload, got {diagnostics:?}",
    );
}

#[test]
fn explicit_type_arg_constraint_pruning_varies_binder_names() {
    let diagnostics = check_source_diagnostics(
        r#"
declare function choose<TNumber extends number>(value: unknown): "number";
declare function choose<TText extends string>(value: unknown): "string";

const selected: "number" = choose<7>("value");
"#,
    );

    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.contains(&2322),
        "explicit number type argument should prune the string-constrained overload, got {diagnostics:?}",
    );
}

#[test]
fn explicit_type_arg_pruning_keeps_multiple_viable_overloads_ordered() {
    let diagnostics = check_source_diagnostics(
        r#"
declare function choose<TSpecific extends "a" | "b">(value: unknown): "specific";
declare function choose<TGeneral extends string>(value: unknown): "general";

const selected: "specific" = choose<"a">(0);
"#,
    );

    let codes = diagnostic_codes(&diagnostics);
    assert!(
        !codes.contains(&2322),
        "multiple viable overloads should keep declaration order, got {diagnostics:?}",
    );
}
