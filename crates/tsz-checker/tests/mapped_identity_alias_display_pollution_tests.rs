//! Identity-style generic mapped aliases must not pollute the global display
//! alias of their argument.
//!
//! An identity homomorphic mapped alias such as `Id<T> = { [K in keyof T]: T[K] }`
//! evaluates to a type that interns to the *same* `TypeId` as its (resolved)
//! argument. tsz keys diagnostic display aliases globally by `TypeId`, so
//! recording `U -> Id<U>` for the result would repaint every later occurrence of
//! the argument `U`. That produced two order-dependent defects in `TS2322`
//! source display:
//!
//! * the identity application rendered as `Id<Id<U>>` (the argument re-wrapped in
//!   its own alias), and
//! * an *unrelated* sibling alias over the same argument — e.g. `Part<U>` checked
//!   later in the same program — rendered as `Part<Id<U>>`.
//!
//! `tsc` never produces either form. The fix suppresses the reverse alias
//! whenever the evaluated result is one of the application's own arguments,
//! regardless of the result's shape (the original guard only fired for non-empty
//! object results, which excluded the union/intersection results these aliases
//! produce).
//!
//! Negative control: a generic alias whose result is a *distinct* type (e.g.
//! `Dict<V> = { [k: string]: V }`) is unaffected and keeps showing its
//! application name. Binder/alias/type-parameter names are varied across cases so
//! the behavior is proven structural, not keyed on an identifier.

use tsz_checker::context::CheckerOptions;
use tsz_common::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
}

fn single(diags: &[Diagnostic], code: u32) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS{code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[0]
}

fn nth(diags: &[Diagnostic], code: u32, index: usize) -> &Diagnostic {
    let matches: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == code).collect();
    assert!(
        matches.len() > index,
        "expected at least {} TS{code}, got: {:?}",
        index + 1,
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    matches[index]
}

/// Core: an identity mapped alias applied to a named union alias must not
/// re-wrap the argument in its own alias name (`Id<Id<U>>`).
#[test]
fn identity_mapped_over_union_alias_does_not_double_wrap() {
    let diags = check_strict(
        "type LeftBox = { a: 1 };\n\
         type RightBox = { b: 2 };\n\
         type Either = LeftBox | RightBox;\n\
         type Echo<S> = { [P in keyof S]: S[P] };\n\
         const bad: number = (null as any as Echo<Either>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        !diag.message_text.contains("Echo<Echo<"),
        "identity mapped alias must not re-wrap its argument; got: {}",
        diag.message_text
    );
    // The argument alias itself must still be reachable (the source is the
    // `Either` union, however it is named), and must never gain a spurious
    // `Echo<` prefix on the inner argument.
    assert!(
        !diag.message_text.contains("Echo<Either>>"),
        "no nested self-application of the argument; got: {}",
        diag.message_text
    );
}

/// Core: evaluating an identity mapped alias over a union must not pollute the
/// display of an unrelated sibling alias over the same union checked afterward.
#[test]
fn identity_mapped_does_not_pollute_sibling_alias_display() {
    let diags = check_strict(
        "type Hot = { readonly a?: number; b: string };\n\
         type Cold = { b: string; readonly c?: boolean };\n\
         type Mix = Hot | Cold;\n\
         type Same<S> = { [P in keyof S]: S[P] };\n\
         type Loose<S> = { [P in keyof S]?: S[P] };\n\
         const first: number = (null as any as Same<Mix>);\n\
         const second: number = (null as any as Loose<Mix>);\n",
    );
    // The `Loose<Mix>` diagnostic is the second TS2322. Its source must be the
    // `Loose<...>` application, never repainted with the sibling `Same` alias.
    let loose = nth(&diags, 2322, 1);
    assert!(
        loose.message_text.contains("Loose<"),
        "sibling alias must keep its own application name; got: {}",
        loose.message_text
    );
    assert!(
        !loose.message_text.contains("Same<"),
        "sibling alias display must not be polluted by the earlier identity alias; got: {}",
        loose.message_text
    );
}

/// Intersection argument: the identity alias over a named intersection alias is
/// the same family and must not double-wrap either.
#[test]
fn identity_mapped_over_intersection_alias_does_not_double_wrap() {
    let diags = check_strict(
        "type Front = { a: 1 };\n\
         type Back = { b: 2 };\n\
         type Both = Front & Back;\n\
         type Clone<U> = { [Q in keyof U]: U[Q] };\n\
         const bad: number = (null as any as Clone<Both>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        !diag.message_text.contains("Clone<Clone<"),
        "identity mapped alias over an intersection must not re-wrap; got: {}",
        diag.message_text
    );
}

/// Negative control: a generic alias whose result is a *distinct* type (an
/// object with an index signature, never equal to its argument) must keep
/// rendering as its application form. This guards against over-suppression.
#[test]
fn distinct_result_alias_keeps_application_name() {
    let diags = check_strict(
        "type Bag<V> = { [k: string]: V };\n\
         const bad: number = (null as any as Bag<string>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text.contains("Bag<string>"),
        "a distinct-result generic alias must keep its application name; got: {}",
        diag.message_text
    );
}

/// Negative control with a two-parameter object alias and varied names — still
/// distinct from its arguments, so the application name is preserved.
#[test]
fn distinct_pair_alias_keeps_application_name() {
    let diags = check_strict(
        "type Couple<X, Y> = { left: X; right: Y };\n\
         const bad: number = (null as any as Couple<number, string>);\n",
    );
    let diag = single(&diags, 2322);
    assert!(
        diag.message_text.contains("Couple<number, string>"),
        "a distinct multi-arg alias must keep its application name; got: {}",
        diag.message_text
    );
}
