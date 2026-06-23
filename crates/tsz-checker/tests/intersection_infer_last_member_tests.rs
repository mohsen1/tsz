//! Regression tests for `infer` extraction from an intersection of function /
//! callable types: tsc settles on the **last** member (last-wins), the basis of
//! the `UnionToIntersection` / `LastOfUnion` idiom.
//!
//! Structural rule (owner: `evaluation/evaluate_rules/infer_pattern.rs`, the
//! intersection-source arm of `match_infer_pattern_inner`): for a
//! function/callable pattern, the matching member of an all-/mixed-callable
//! intersection that binds the `infer` variable is the LAST one — for both
//! covariant-return (`(()=>"x")&(()=>"y")&(()=>"z")` extends `() => infer R` →
//! `"z"`) and contravariant-parameter (`((k:"a")=>void)&((k:"b")=>void)` extends
//! `(k: infer K) => void` → `"b"`) positions. The arm previously accepted the
//! FIRST matching member, mis-binding to `"x"`/`"a"` and drawing a spurious
//! TS2322. Non-signature patterns keep declaration order (first
//! structurally-matching constituent), e.g. picking the callable out of a
//! `callable & brand` intersection.

use tsz_checker::test_utils::check_source_codes;

fn codes(source: &str) -> Vec<u32> {
    let mut c = check_source_codes(source);
    c.sort_unstable();
    c.dedup();
    c
}

#[test]
fn covariant_return_infer_picks_last_member() {
    assert!(
        codes(
            r#"
type I = (() => "x") & (() => "y") & (() => "z");
type L = I extends () => infer R ? R : never;
const a: "z" = (null as any as L);
"#,
        )
        .is_empty(),
        "infer R from an intersection of return-typed functions should be the last (\"z\")",
    );
}

#[test]
fn contravariant_param_infer_picks_last_member() {
    assert!(
        codes(
            r#"
type I = ((k: "a") => void) & ((k: "b") => void);
type P = I extends (k: infer K) => void ? K : never;
const p: "b" = (null as any as P);
"#,
        )
        .is_empty(),
        "infer K from an intersection of param-typed functions should be the last (\"b\")",
    );
}

#[test]
fn last_of_union_idiom_resolves() {
    // The full type-fest LastOfUnion / UnionToTuple idiom.
    assert!(
        codes(
            r#"
type UnionToIntersection<U> =
  (U extends any ? (k: U) => void : never) extends (k: infer I) => void ? I : never;
type LastOf<U> =
  UnionToIntersection<U extends any ? () => U : never> extends () => infer R ? R : never;
type X = LastOf<"a" | "b" | "c">;
const x: "c" = (null as any as X);
"#,
        )
        .is_empty(),
        "LastOf<\"a\"|\"b\"|\"c\"> should resolve to \"c\"",
    );
}

#[test]
fn two_member_intersection_picks_last() {
    assert!(
        codes(
            r#"
type I = (() => "x") & (() => "y");
type L = I extends () => infer R ? R : never;
const a: "y" = (null as any as L);
"#,
        )
        .is_empty(),
        "two-member function intersection infer should be the last (\"y\")",
    );
}

#[test]
fn last_wins_negative_control_not_first() {
    // The result must NOT be the first member: assigning the first literal is an
    // error, proving last-wins rather than accept-anything.
    assert_eq!(
        codes(
            r#"
type I = (() => "x") & (() => "y") & (() => "z");
type L = I extends () => infer R ? R : never;
const bad: "x" = (null as any as L);
"#,
        ),
        vec![2322],
        "L is \"z\", so assigning to \"x\" must error",
    );
}

#[test]
fn object_intersection_still_gathers_all_members() {
    // Control: a non-signature (object) pattern must still collect properties
    // from every intersection member.
    assert!(
        codes(
            r#"
type I = { a: 1 } & { b: 2 };
type R = I extends { a: infer A; b: infer B } ? [A, B] : never;
const r: [1, 2] = (null as any as R);
"#,
        )
        .is_empty(),
        "object-pattern intersection infer must gather all members",
    );
}

#[test]
fn callable_and_brand_intersection_still_picks_callable() {
    // Control: a `callable & brand` intersection still binds from the callable
    // constituent (declaration order is irrelevant when only one matches).
    assert!(
        codes(
            r#"
type Fn = ((x: number) => void) & { brand: "z" };
type P = Fn extends (x: infer A) => void ? A : never;
const p: number = (null as any as P);
"#,
        )
        .is_empty(),
        "callable & brand intersection should bind from the callable member",
    );
}
