//! Regression tests: an argument-pinned bare return type parameter that
//! resolves to `unknown` must not be silently promoted to the contextual
//! (expected) type.
//!
//! Structural rule: when a generic call's return type is a bare type
//! parameter `T` that a concrete (non-context-sensitive) value argument
//! directly pins, argument inference owns `T` — mirroring the solver's
//! #14262 `value_arg_seeded_bare_return_param` rule, which already keeps an
//! `as never` cast from clamping such a parameter. The checker's contextual-
//! return finalization (`finalize_generic_call_result` /
//! `get_type_of_call_expression_inner`) had an unguarded fallback: whenever
//! the argument-inferred return type structurally contained `unknown` and
//! the contextually-instantiated return type happened to be assignable to
//! the expected type, it replaced the call's result with the contextual
//! type outright — without checking that the argument actually supported
//! that substitution. `declare const w: unknown; const s: string =
//! generic(w)` (where `generic<T>(x: T): T`) therefore silently compiled:
//! `T` was overwritten from the correct `unknown` to `string` because
//! `string` happens to satisfy the target, even though the real argument
//! (`unknown`) does not support it. tsc reports TS2322 here.
//!
//! Anti-hardcoding: the structural rule is "the return type parameter is
//! the naked type of a value parameter pinned by a concrete argument whose
//! actual type does not satisfy the contextually-implied parameter type",
//! so the tests vary binder names, contextual-typing positions (variable
//! declaration, assignment, return statement, multi-declarator), and
//! wrapper shapes rather than matching a specific identifier or message.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_code_message_refs};

fn assert_has_code(source: &str, code: u32, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "{context}: expected TS{code}, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

fn assert_no_code(source: &str, code: u32, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "{context}: expected no TS{code}, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn unknown_argument_pinned_return_still_reports_on_var_decl_initializer() {
    assert_has_code(
        r#"
declare function generic<T>(x: T): T
declare const w: unknown
const s: string = generic(w)
"#,
        2322,
        "unknown-typed value argument must not be promoted to the contextual type",
    );
}

#[test]
fn unknown_argument_pinned_return_still_reports_on_assignment() {
    assert_has_code(
        r#"
declare function generic<T>(x: T): T
declare const w: unknown
let s: string
s = generic(w)
"#,
        2322,
        "assignment-expression contextual position must not clamp the argument-pinned return",
    );
}

#[test]
fn unknown_argument_pinned_return_still_reports_on_return_statement() {
    assert_has_code(
        r#"
declare function generic<T>(x: T): T
declare const w: unknown
function f(): string {
    return generic(w)
}
"#,
        2322,
        "return-statement contextual position must not clamp the argument-pinned return",
    );
}

#[test]
fn unknown_argument_pinned_return_still_reports_with_renamed_binders() {
    assert_has_code(
        r#"
declare function identity<ZZZ>(value: ZZZ): ZZZ
declare const anyUnknown: unknown
const out: number = identity(anyUnknown)
"#,
        2322,
        "renamed binders must behave identically",
    );
}

#[test]
fn unknown_argument_pinned_return_still_reports_through_wrapper_shape() {
    assert_has_code(
        r#"
declare function wrap<T>(x: T): { value: T }
declare const w: unknown
const out: { value: string } = wrap(w)
"#,
        2322,
        "a wrapped (structural) return shape must still surface the unresolved member",
    );
}

#[test]
fn multi_declarator_second_binding_still_reports() {
    assert_has_code(
        r#"
declare function generic<T>(x: T): T
declare const w: unknown
const s: string = generic(w), n: number = generic(w)
"#,
        2322,
        "each declarator in a multi-declarator statement is checked independently",
    );
}

#[test]
fn call_argument_position_keeps_reporting_ts2345() {
    // Negative control mirroring the existing call-argument path, which was
    // never affected by this bug (contextual typing there already reports
    // TS2345). Kept here so the family is documented together.
    assert_has_code(
        r#"
declare function generic<T>(x: T): T
declare const w: unknown
declare function target(v: string): void
target(generic(w))
"#,
        2345,
        "call-argument contextual position already reported correctly",
    );
}

#[test]
fn literal_argument_still_gets_contextual_literal_preservation() {
    // Positive control: when the pinning argument's actual type DOES satisfy
    // the contextually-implied parameter type (a string literal argument
    // against a target requiring a narrower literal type), the contextual
    // return type is still legitimately adopted and no spurious TS2322 fires.
    assert_no_code(
        r#"
type DooDad = "SOMETHING" | "ELSE"
declare function identity<T>(x: T): T
const v: DooDad = identity("ELSE")
"#,
        2322,
        "a supporting literal argument must keep the contextual-return literal preservation",
    );
}

#[test]
fn any_argument_is_unaffected() {
    // Negative control: an `any`-typed pinning argument is assignable to
    // anything, so `contextual_params_fit_args` naturally holds and the call
    // still type-checks cleanly (matches tsc: `any` silences the mismatch).
    assert_no_code(
        r#"
declare function generic<T>(x: T): T
declare const a: any
const s: string = generic(a)
"#,
        2322,
        "an any-typed argument must not spuriously report",
    );
}
