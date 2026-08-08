//! An arrow function assigned to an object type with two or more overloaded,
//! same-arity GENERIC call signatures must contextually type its parameters
//! from a single shared type-parameter identity — not from each overload's
//! own, textually-identical-but-distinct binder.
//!
//! Structural rule: `tsc`'s overload-merge machinery
//! (`getIntersectedSignatures`/`createTypeMapper`) treats same-arity generic
//! overloads as sharing one canonical type-parameter list (the first
//! signature's), mapping every later signature's own type parameters onto it
//! before combining parameter types. tsz's `ParameterForCallExtractor`
//! (`crates/tsz-solver/src/contextual/extractors_for_call.rs`) previously
//! collected each overload's own, distinct type-parameter `TypeId` and unioned
//! them without reduction, producing a displayed `T | T` and — because the two
//! `T`s are different binders — a subsequent relation check against that
//! merged type failed even though a single bare `T` would have passed,
//! producing a spurious `TS2322` (issue #16950).
//!
//! A residual gap remains when the merged overloads' RETURN types disagree
//! (`<T>(x:T):string` vs `<T>(x:T):number`) — tracked separately as #16952
//! (generic-to-generic signature erasure) and deliberately not asserted clean
//! here.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_count};

fn options() -> CheckerOptions {
    CheckerOptions::default()
}

/// The bug witness: two same-arity generic overloads that agree on their
/// return type. The identity arrow's contextual parameter type must be the
/// single shared `T`, not `T | T`.
#[test]
fn identity_arrow_against_agreeing_generic_overloads_is_clean() {
    let source = r#"
var f3: {
    <T>(x: T): T;
    <T>(x: T): T;
} = (x) => x;
"#;
    let diags = check_source(source, "identity.ts", options());
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "identity arrow against agreeing same-arity generic overloads must \
         not report TS2322 (merged contextual type must be a single shared \
         T, not T | T); got: {diags:?}"
    );
}

/// Same shape with two type parameters, proving the mapping is positional
/// (not name-keyed only) across the full parameter list.
#[test]
fn identity_arrow_against_agreeing_two_type_param_overloads_is_clean() {
    let source = r#"
var f5: {
    <T, U>(x: T, y: U): T;
    <T, U>(x: T, y: U): T;
} = (x, y) => x;
"#;
    let diags = check_source(source, "two_params.ts", options());
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "two-type-parameter agreeing overloads must not report TS2322; got: \
         {diags:?}"
    );
}

/// Structural, name-independent positive check: renaming both overloads'
/// shared binder must not change the outcome.
#[test]
fn identity_arrow_against_renamed_binder_generic_overloads_is_clean() {
    let source = r#"
var f3: {
    <Elem>(x: Elem): Elem;
    <Elem>(x: Elem): Elem;
} = (x) => x;
"#;
    let diags = check_source(source, "renamed.ts", options());
    assert_eq!(
        diagnostic_count(&diags, 2322),
        0,
        "renaming the shared type-parameter binder must not change the \
         outcome; got: {diags:?}"
    );
}

/// Negative control: `x`'s merged contextual type is a bare, opaque `T` — a
/// member access that no unconstrained type parameter can have must still be
/// rejected. Guards that merging same-arity generic overloads does not widen
/// `x` to `any` or otherwise turn off real member-existence checking.
#[test]
fn member_access_on_shared_type_param_still_errors() {
    let source = r#"
var f3: {
    <T>(x: T): T;
    <T>(x: T): T;
} = (x) => x.toFixed();
"#;
    let diags = check_source(source, "mismatch.ts", options());
    assert!(
        diagnostic_count(&diags, 2339) > 0,
        "a member access with no counterpart on an unconstrained shared T \
         must still fail; got: {diags:?}"
    );
}

/// Negative control: mismatched type-parameter arity across overloads is a
/// different shape (not the same-arity shared-identity case) and must keep
/// its prior, non-mapped behavior rather than panicking or looping.
#[test]
fn mismatched_type_param_arity_overloads_do_not_crash() {
    let source = r#"
var f4: {
    <T>(x: T): string;
    <T, U>(x: T, y: U): number;
} = (x) => x;
"#;
    let _diags = check_source(source, "mismatched_arity.ts", options());
}
