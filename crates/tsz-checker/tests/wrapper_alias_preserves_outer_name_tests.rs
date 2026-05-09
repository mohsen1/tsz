//! Tests that `type Wrapper<V> = Inner<V>` preserves the outer alias name in
//! TS2322 diagnostics when `Inner` resolves to a structural type (interface,
//! class, or another non-mapped type alias).
//!
//! Structural rule: when an `Application(Wrapper, args)` is the diagnostic
//! source/target and the wrapper alias body is `Application(Inner, ...)`, the
//! display only unfolds to `Inner<...>` if `Inner` is itself a type alias
//! whose body is a *mapped type*. For type-alias-of-interface,
//! type-alias-of-class, or type-alias-of-non-mapped-alias chains, the
//! diagnostic preserves the outer wrapper.
//!
//! Conformance fixture covered by this rule:
//!   - `compiler/varianceReferences.ts` — `vs1 = vs12` and `vds1 = vds12`
//!     emit `Type 'VarianceShape<2 | 1>' is not assignable to type
//!     'VarianceShape<1>'` and `Type 'VarianceDeepShape<2 | 1>' is not
//!     assignable to type 'VarianceDeepShape<1>'` rather than `Shape<...>` /
//!     `Level1of3Shape<...>`.
//!
//! Counterpart preserved by the rule (the unfold case):
//!   - `conformance/types/mapped/mappedTypeWithAny.ts` — `arr =
//!     indirectArrayish` continues to display `Objectish<any>` because the
//!     body alias `Objectish<T>` has a mapped-type body.

use tsz_checker::test_utils::check_source_strict_messages as check_strict;

/// `type Wrapper<V> = Iface<V>` over an interface preserves the wrapper name
/// in TS2322 messages.
#[test]
fn ts2322_wrapper_alias_over_interface_preserves_outer_alias_name() {
    let source = r#"
interface Iface<Value> {
  value: Value;
}

type Wrapper<Value> = Iface<Value>;

declare let a: Wrapper<1>;
declare let b: Wrapper<1 | 2>;

a = b;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `a = b`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'Wrapper<"),
        "TS2322 source/target must keep the outer wrapper alias name `Wrapper<...>`. \
         Got: {msg:?}"
    );
    assert!(
        !msg.contains("'Iface<"),
        "TS2322 must NOT unfold to the inner `Iface<...>` when the inner def is an \
         interface. Got: {msg:?}"
    );
}

/// Same rule with a different identifier — verifies the fix is not hardcoded
/// to a specific name.
#[test]
fn ts2322_wrapper_alias_over_interface_preserves_outer_alias_name_alt_names() {
    let source = r#"
interface Box<X> {
  value: X;
}

type WrappedBox<X> = Box<X>;

declare let p: WrappedBox<"hi">;
declare let q: WrappedBox<"hi" | "hello">;

p = q;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `p = q`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'WrappedBox<"),
        "TS2322 source/target must keep the outer wrapper alias `WrappedBox<...>` \
         regardless of identifier choice. Got: {msg:?}"
    );
    assert!(
        !msg.contains("'Box<"),
        "TS2322 must NOT unfold to inner `Box<...>` for type-alias-of-interface. \
         Got: {msg:?}"
    );
}

/// `type Wrapper<V> = Mid<V>` where `Mid` is itself a type-alias-of-interface
/// chain still preserves the OUTER `Wrapper<...>` name. Without this we would
/// see `Mid<...>` (the wrong intermediate alias) even though no mapped type
/// is involved anywhere on the chain.
#[test]
fn ts2322_wrapper_alias_over_alias_of_interface_preserves_outer_alias_name() {
    let source = r#"
interface Leaf<V> {
  value: V;
}

type Mid<V> = Leaf<V>;
type Outer<V> = Mid<V>;

declare let a: Outer<1>;
declare let b: Outer<1 | 2>;

a = b;
"#;
    let diags = check_strict(source);
    let ts2322: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 for `a = b`; got: {diags:?}"
    );
    let msg = &ts2322[0].1;
    assert!(
        msg.contains("'Outer<"),
        "TS2322 source/target must keep the outer wrapper alias `Outer<...>` even \
         when the body is itself a type-alias-of-type-alias chain that ultimately \
         resolves through interfaces. Got: {msg:?}"
    );
    assert!(
        !msg.contains("'Mid<") && !msg.contains("'Leaf<"),
        "TS2322 must NOT unfold to intermediate `Mid<...>` or `Leaf<...>` for \
         type-alias-of-non-mapped-alias chains. Got: {msg:?}"
    );
}

/// Counterpart: `type Wrapper<U> = Mapped<U>` where `Mapped<T> = { [K in
/// keyof T]: T[K] }` continues to unfold to `Mapped<...>` to match tsc's
/// `mappedTypeWithAny.ts` behavior. This guards the rule from regressing
/// into "never unfold."
#[test]
fn ts2740_wrapper_alias_over_mapped_alias_unfolds_to_inner_mapped_alias() {
    let source = r#"
type Mapped<T extends unknown> = { [K in keyof T]: T[K] };
type IndirectArrayish<U extends unknown[]> = Mapped<U>;

function bar(indirectArrayish: IndirectArrayish<any>) {
    let arr: any[];
    arr = indirectArrayish;
}
"#;
    let diags = check_strict(source);
    let any_assignability_codes = [2322u32, 2740];
    let mismatch: Vec<&(u32, String)> = diags
        .iter()
        .filter(|(c, _)| any_assignability_codes.contains(c))
        .collect();
    assert!(
        !mismatch.is_empty(),
        "expected an assignability error for `arr = indirectArrayish`; got: {diags:?}"
    );
    let any_msg_unfolded = mismatch.iter().any(|(_, m)| m.contains("'Mapped<"));
    assert!(
        any_msg_unfolded,
        "Wrapper alias whose body is itself a mapped-type alias must unfold to \
         `Mapped<...>` (matches tsc's `Objectish<any>` behavior). Got: {mismatch:?}"
    );
    let any_kept_wrapper = mismatch
        .iter()
        .any(|(_, m)| m.contains("'IndirectArrayish<"));
    assert!(
        !any_kept_wrapper,
        "Wrapper alias whose body is a mapped-type alias should NOT preserve the \
         outer wrapper name in this case. Got: {mismatch:?}"
    );
}
