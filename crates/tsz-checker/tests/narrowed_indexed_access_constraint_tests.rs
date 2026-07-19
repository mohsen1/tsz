//! Regression: a conditional whose check operand is a naked type parameter used
//! as the **index** of an indexed access narrows that index inside the true
//! branch (`tsc`'s `getConditionalFlowTypeOfType` produces a `SubstitutionType`),
//! turning `T[k]` into `T[Substitution(k, k & Extends)]`. The narrowed access
//! must satisfy a downstream type-parameter constraint exactly as the un-narrowed
//! `T[k]` does — the narrowing only restricts the key, and for an index-signature
//! object the value type is unchanged.
//!
//! Witness: zod `types.ts:1833` — `ZodObject.partial` returns
//! `{ [k in keyof T]: k extends keyof Mask ? ZodOptional<T[k]> : T[k] }`, where
//! `ZodOptional<T extends ZodTypeAny>` forced a false TS2344 on the narrowed
//! `T[k]`. Verified against tsc 7.0.2: the minimal forms below are accepted, and
//! the negative controls (genuinely-failing value type) are rejected.
//!
//! Binder names, arities, and the wrapper/shape identifiers are varied across
//! cases so the guard follows structure, not identifier text.

use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::check_source_with_libs_code_messages;

fn codes(source: &str) -> Vec<u32> {
    let libs = tsz_checker::test_utils::load_default_lib_files();
    let opts = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_source_with_libs_code_messages(source, "test.ts", opts, &libs)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// The mined witness (binders renamed off zod's): a conditional narrowing the
/// index `k` in `Wrap<T[k]>` where the object's index-signature value satisfies
/// `Wrap`'s constraint. No TS2344.
#[test]
fn narrowed_index_into_index_signature_shape_no_ts2344() {
    let c = codes(
        r#"
abstract class Node<A = any, B = any, C = A> { readonly _a!: A; readonly _b!: B; readonly _c!: C; }
type AnyNode = Node<any, any, any>;
class Wrap<T extends AnyNode> { constructor(public inner: T) {} }
type Shape = { [k: string]: AnyNode };
type M<T extends Shape, k extends keyof T> = k extends string ? Wrap<T[k]> : never;
export {};
"#,
    );
    assert!(
        !c.contains(&2344),
        "no TS2344 expected — T[k] narrowed to T[k & string] reduces through the \
         string index signature to AnyNode, which satisfies Wrap's constraint. Got: {c:?}"
    );
}

/// Different binder names, arities, and a key-remapped mapped-type form (mirrors
/// the actual `ZodObject.partial` return). No TS2344.
#[test]
fn narrowed_index_in_mapped_type_renamed_binders_no_ts2344() {
    let c = codes(
        r#"
abstract class Cell<X = any, Y = any> { readonly _x!: X; readonly _y!: Y; }
type AnyCell = Cell<any, any>;
class Boxed<E extends AnyCell> { constructor(public v: E) {} }
type Bag = { [p: string]: AnyCell };
type Part<S extends Bag, Mask> = { [p in keyof S]: p extends keyof Mask ? Boxed<S[p]> : S[p] };
export {};
"#,
    );
    assert!(
        !c.contains(&2344),
        "no TS2344 expected — S[p] narrowed by `p extends keyof Mask` still reduces \
         through Bag's index signature to AnyCell. Got: {c:?}"
    );
}

/// Positive control: the un-narrowed `Wrap<T[k]>` (no conditional) was always
/// clean and must stay clean — locks the mainline reduction the narrowed path
/// now mirrors.
#[test]
fn plain_index_without_conditional_still_clean() {
    let c = codes(
        r#"
abstract class Node<A = any, B = any, C = A> { readonly _a!: A; readonly _b!: B; readonly _c!: C; }
type AnyNode = Node<any, any, any>;
class Wrap<T extends AnyNode> { constructor(public inner: T) {} }
type Shape = { [k: string]: AnyNode };
type M<T extends Shape, k extends keyof T> = Wrap<T[k]>;
export {};
"#,
    );
    assert!(
        !c.contains(&2344),
        "no TS2344 expected for the plain (un-narrowed) indexed access. Got: {c:?}"
    );
}

/// Negative control: the object's index-signature value is `string`, which does
/// NOT satisfy `Wrap`'s constraint. The narrowing must not blanket-suppress —
/// TS2344 is still required (matches tsc 7.0.2).
#[test]
fn narrowed_index_failing_value_type_still_ts2344() {
    let c = codes(
        r#"
abstract class Node<A = any, B = any, C = A> { readonly _a!: A; readonly _b!: B; readonly _c!: C; }
type AnyNode = Node<any, any, any>;
class Wrap<T extends AnyNode> { constructor(public inner: T) {} }
type PlainShape = { [k: string]: string };
type MBad<T extends PlainShape, k extends keyof T> = k extends string ? Wrap<T[k]> : never;
export {};
"#,
    );
    assert!(
        c.contains(&2344),
        "TS2344 expected — T[k] narrowed still reduces to `string`, which does not \
         satisfy Wrap's `extends AnyNode` constraint. Got: {c:?}"
    );
}

/// Negative control: an UNRELATED conditional check operand (`U`, not the index
/// `k`) does not narrow `k`, so the plain reduction applies — and here the value
/// type genuinely satisfies, so it stays clean. Confirms the fix is driven by the
/// index narrowing, not the mere presence of a conditional.
#[test]
fn unrelated_conditional_operand_no_ts2344() {
    let c = codes(
        r#"
abstract class Node<A = any, B = any, C = A> { readonly _a!: A; readonly _b!: B; readonly _c!: C; }
type AnyNode = Node<any, any, any>;
class Wrap<T extends AnyNode> { constructor(public inner: T) {} }
type Shape = { [k: string]: AnyNode };
type M<T extends Shape, k extends keyof T, U> = U extends string ? Wrap<T[k]> : never;
export {};
"#,
    );
    assert!(
        !c.contains(&2344),
        "no TS2344 expected — the conditional check operand U is unrelated to the \
         index k. Got: {c:?}"
    );
}
