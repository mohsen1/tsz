//! Tests for tsc-style display of generic type-alias applications whose body
//! is a Conditional (or `IndexedAccess`) type that does NOT reduce because of
//! free type parameters.
//!
//! Structural rule: when an `Application(alias, args)` is the diagnostic
//! target and the alias body is a Conditional or `IndexedAccess` type, expand
//! to the evaluated form only if evaluation reduces to a non-conditional /
//! non-indexed-access shape. If the conditional/indexed-access stays in
//! place (e.g. free type parameters block reduction), preserve the alias
//! display.
//!
//! Conformance test fixed by this rule:
//! - `conditionalTypeVarianceBigArrayConstraintsPerformance.ts`
//!
//! tsc shows `Type 'Stuff<U>' is not assignable to type 'Stuff<T>'`; we used
//! to expand the target to its conditional body
//! `T extends keyof IntrinsicElements ? IntrinsicElements[T] : any`.

use crate::test_utils::check_source_strict_messages as check_strict;

/// `Stuff<T>` and `Stuff<U>` are Application of an alias whose body is a
/// Conditional with free type parameter `T`. Reduction stalls. tsc shows
/// the alias on both sides.
#[test]
fn ts2322_conditional_alias_with_free_type_param_keeps_alias_target_display() {
    let source = r#"
type Stuff<T> =
    T extends string ? T : never;
function F<T, U>(p1: Stuff<T>, p2: Stuff<U>) {
    p1 = p2;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `p1 = p2`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'Stuff<T>'"),
        "TS2322 target must keep the alias `Stuff<T>` when the conditional \
         does not reduce. Got: {msg:?}"
    );
    assert!(
        !msg.contains("T extends string ? T : never"),
        "TS2322 target must NOT expand to the conditional body when reduction \
         stalls on a free type parameter. Got: {msg:?}"
    );
}

/// Same rule with a different type-parameter name to verify the fix is not
/// hardcoded to a specific name.
#[test]
fn ts2322_conditional_alias_with_free_type_param_keeps_alias_target_display_alt_name() {
    let source = r#"
type Wrap<X> =
    X extends number ? X : never;
function F<X, Y>(p1: Wrap<X>, p2: Wrap<Y>) {
    p1 = p2;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `p1 = p2`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'Wrap<X>'"),
        "TS2322 target must keep the alias `Wrap<X>` when the conditional \
         does not reduce. Got: {msg:?}"
    );
    assert!(
        !msg.contains("X extends number ? X : never"),
        "TS2322 target must NOT expand to the conditional body when reduction \
         stalls on a free type parameter. Got: {msg:?}"
    );
}
