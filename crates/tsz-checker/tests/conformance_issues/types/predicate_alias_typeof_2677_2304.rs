//! Type-predicate signatures in function-type position must resolve the same way
//! tsc resolves them: (1) an alias-typed predicate parameter is expanded before
//! the predicate-assignability relation (TS2677), and (2) the signature's value
//! parameters are in scope for `typeof <param>` in the predicate's asserted type
//! (TS2304). Both were false positives in `tsz`. (#14231, #14229)

use super::super::core::*;

/// #14231 (TS2677): when a type predicate's parameter type is an alias head
/// (`Alias<T> = keyof T`), the predicate-assignability relation must run on the
/// resolved alias body, not the opaque `Lazy`/`Application` reference. `tsz`
/// previously related the unresolved head and wrongly reported TS2677.
#[test]
fn type_predicate_through_alias_no_ts2677() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Alias<T> = keyof T;
let g: <T>(p: Alias<T>) => p is keyof T;
type A = string;
let g2: (p: A) => p is string;
export {};
"#,
    );
    assert!(
        !has_error(&diagnostics, 2677),
        "no TS2677 expected — the alias-typed predicate parameter must be resolved \
         before the predicate-assignability relation. Actual: {diagnostics:#?}"
    );
    // Negative control: a predicate that genuinely doesn't assign must still error.
    let neg = compile_and_get_diagnostics(
        r#"
let g3: (p: string) => p is number;
export {};
"#,
    );
    assert!(
        has_error(&neg, 2677),
        "TS2677 expected — `p is number` is not assignable to `p: string`. \
         Actual: {neg:#?}"
    );
}

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
