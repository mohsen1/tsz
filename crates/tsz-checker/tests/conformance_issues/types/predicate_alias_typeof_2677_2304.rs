//! A type-predicate signature's value parameters are in scope for every type
//! position of that signature, including a `typeof <param>` inside the
//! predicate's asserted type. `tsz` failed to seed the parameter scope before
//! lowering the function-type node, so `typeof a` reported a false TS2304. (#14229)

use super::super::core::*;

/// #14229 (TS2304): `typeof <param>` inside a function-type alias's type-predicate
/// asserted type must resolve the parameter, which is in scope for every type
/// position of the signature. `tsz` failed to seed the parameter scope before
/// lowering the function-type node, so `typeof a` reported a false TS2304.
#[test]
fn typeof_param_in_predicate_asserted_type_no_ts2304() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Guard = (a: { z: string }) => a is typeof a & { y: boolean };
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2304),
        "no TS2304 expected — the signature's value parameter `a` is in scope for \
         the predicate's asserted type (`typeof a`). Actual: {diagnostics:#?}"
    );
    // Negative control: an undeclared name in the asserted type must still error.
    let neg = compile_and_get_diagnostics(
        r#"
type Guard2 = (a: { z: string }) => a is typeof undeclared & { y: boolean };
export {};
"#,
    );
    assert!(
        has_error(&neg, 2304),
        "TS2304 expected — `typeof undeclared` references an undeclared name. \
         Actual: {neg:#?}"
    );
}
