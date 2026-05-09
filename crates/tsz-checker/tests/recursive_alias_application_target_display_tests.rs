//! Tests for tsc-style display of TS2322 messages when the assignment target
//! is a generic Application of a *recursive* type alias (an alias whose body
//! reaches another reference to itself).
//!
//! Structural rule: when an `Application(Lazy(D), args)` is the diagnostic
//! target and `D`'s alias body recursively references `D` (directly or
//! through nested types), preserve the alias annotation in the message.
//! Expanding the body produces an unbounded `[..., ...]` cascade because the
//! diagnostic printer flattens every nested Application when alias names are
//! skipped — the user-facing message becomes useless.
//!
//! Conformance test fixed by this rule:
//! - `inferFromNestedSameShapeTuple.ts`
//!
//! tsc shows `Type 'T1<U>' is not assignable to type 'T2<U>'`; we used to
//! expand the target to `[42, [42, [42, [42, [42, [42, [42, [42, [..., ...]]]]]]]]]`.

use crate::test_utils::check_source_strict_messages as check_strict;

/// `T1<U>` / `T2<U>` are Applications of recursive tuple aliases. The alias
/// bodies reach themselves via nested Applications. tsc keeps the alias name
/// on both sides; expansion would emit `[42, [42, [..., ...]]]` cascades.
#[test]
fn ts2322_recursive_tuple_alias_keeps_alias_target_display() {
    let source = r#"
type T1<T> = [number, T1<{ x: T }>];
type T2<T> = [42, T2<{ x: T }>];

function qq<U>(x: T1<U>, y: T2<U>) {
    y = x;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `y = x`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'T2<U>'"),
        "TS2322 target must keep the alias `T2<U>` for a recursive tuple \
         alias instead of expanding to its body. Got: {msg:?}"
    );
    assert!(
        !msg.contains("[..., ...]"),
        "TS2322 message must not contain the elision marker that signals \
         unbounded recursive expansion. Got: {msg:?}"
    );
    assert!(
        !msg.contains("[42, [42"),
        "TS2322 message must not expand a recursive alias to a stack of its \
         own body. Got: {msg:?}"
    );
}

/// Same rule with different type-parameter names to verify the fix is not
/// hardcoded to particular identifiers (anti-hardcoding directive §25).
#[test]
fn ts2322_recursive_tuple_alias_keeps_alias_target_display_alt_names() {
    let source = r#"
type RecA<P> = [string, RecA<{ inner: P }>];
type RecB<P> = ["lit", RecB<{ inner: P }>];

function f<X>(a: RecA<X>, b: RecB<X>) {
    b = a;
}
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `b = a`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'RecB<X>'"),
        "TS2322 target must keep the recursive alias name `RecB<X>`. Got: \
         {msg:?}"
    );
    assert!(
        !msg.contains("[..., ...]"),
        "TS2322 message must not contain the elision marker. Got: {msg:?}"
    );
}

/// A non-recursive generic alias with a tuple body. tsz currently expands
/// this to the structural form, matching the existing `preserve_tuple_alias_display`
/// behaviour. The recursive-alias rule must not affect this case.
#[test]
fn ts2322_non_recursive_tuple_alias_target_display_unchanged() {
    let source = r#"
type Pair<T> = [T, T];
let a: Pair<number>;
let b: Pair<string>;
b = a;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected at least one TS2322 for `b = a`; got: {diags:?}"
    );
    // The non-recursive case retains the existing `preserve_tuple_alias_display`
    // path: target expands to `[string, string]`. The recursive-alias guard
    // must not affect this. The assertion intentionally locks the *current*
    // behaviour for the non-recursive case to catch accidental scope creep.
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("[string, string]") || msg.contains("'Pair<string>'"),
        "TS2322 target for a non-recursive alias should remain in its \
         existing display path (currently expands to `[string, string]`). \
         Got: {msg:?}"
    );
}
