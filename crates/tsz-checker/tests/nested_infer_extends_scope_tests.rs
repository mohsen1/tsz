//! Tests for nested `infer Y extends C2` declarations nested inside the
//! constraint of an outer `infer X extends C1` (issue #6770).
//!
//! Structural rule: when a conditional type's extends clause contains an
//! `infer X extends Constraint`, every `infer Y extends C2` nested anywhere
//! inside Constraint is also bound in the true branch's scope. tsc accepts
//! references to `Y` from the true branch; previously tsz emitted TS2304.
//!
//! These tests vary the binder names (`V`/`N`, `A`/`B`, `P`/`Q`/`R`) and the
//! containing shape (type literal, tuple, array, function, union, mapped
//! type, deeper nesting) to verify the fix expresses the structural rule and
//! is not a fingerprint of the reported repro.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_strict;

fn ts2304(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags.iter().filter(|d| d.code == 2304).collect()
}

fn assert_no_2304(label: &str, diags: &[Diagnostic]) {
    let errs = ts2304(diags);
    assert!(
        errs.is_empty(),
        "{label}: unexpected TS2304 diagnostics: {:?}",
        errs.iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn nested_infer_in_type_literal_constraint_is_visible_in_true_branch() {
    // Reported repro from issue #6770.
    let source = r#"
type DeepNumber<T> =
  T extends { value: infer V extends { nested: infer N extends number } }
    ? N
    : never;

type DN1 = DeepNumber<{ value: { nested: 42 } }>;
type DN2 = DeepNumber<{ value: { nested: "str" } }>;
"#;

    assert_no_2304("type-literal constraint", &check_source_strict(source));
}

#[test]
fn nested_infer_visible_under_renamed_binders() {
    // Same rule, different binder names — verifies the fix is not keyed on
    // the literal identifiers `V`/`N`.
    let source = r#"
type DeepAlias<X> =
  X extends { v: infer A extends { n: infer B extends string } }
    ? B
    : never;

type R = DeepAlias<{ v: { n: "ok" } }>;
"#;

    assert_no_2304("renamed binders", &check_source_strict(source));
}

#[test]
fn nested_infer_in_array_constraint_visible() {
    let source = r#"
type ArrayInner<T> =
  T extends (infer V extends Array<infer N extends number>)
    ? [V, N]
    : never;

type R = ArrayInner<number[]>;
"#;

    assert_no_2304("array constraint", &check_source_strict(source));
}

#[test]
fn nested_infer_in_tuple_constraint_visible() {
    let source = r#"
type TupleInner<T> =
  T extends infer V extends readonly [infer A extends string, infer B extends number]
    ? [A, B]
    : never;

type R = TupleInner<readonly ["x", 1]>;
"#;

    assert_no_2304("tuple constraint", &check_source_strict(source));
}

#[test]
fn nested_infer_in_function_return_constraint_visible() {
    let source = r#"
type FnInner<T> =
  T extends infer F extends (...args: any[]) => infer R extends number
    ? R
    : never;

type R = FnInner<() => 42>;
"#;

    assert_no_2304("function return constraint", &check_source_strict(source));
}

#[test]
fn nested_infer_in_function_parameter_constraint_visible() {
    let source = r#"
type FnParam<T> =
  T extends infer F extends (x: infer P extends string) => unknown
    ? P
    : never;

type R = FnParam<(x: "y") => void>;
"#;

    assert_no_2304(
        "function parameter constraint",
        &check_source_strict(source),
    );
}

#[test]
fn nested_infer_in_union_constraint_visible() {
    let source = r#"
type UnionInner<T> =
  T extends infer V extends string | (infer N extends number)
    ? N
    : never;

type R = UnionInner<1 | "s">;
"#;

    assert_no_2304("union constraint", &check_source_strict(source));
}

#[test]
fn three_level_nested_infer_visible() {
    // Verifies recursion is unbounded depth, not a special case for one level.
    let source = r#"
type Triple<T> =
  T extends { a: infer A extends { b: infer B extends { c: infer C extends number } } }
    ? [A, B, C]
    : never;

type R = Triple<{ a: { b: { c: 7 } } }>;
"#;

    assert_no_2304("three-level nesting", &check_source_strict(source));
}

#[test]
fn sibling_nested_infers_both_visible() {
    let source = r#"
type Siblings<T> =
  T extends { x: infer V extends { l: infer L extends number; r: infer R extends string } }
    ? [V, L, R]
    : never;

type R = Siblings<{ x: { l: 1; r: "a" } }>;
"#;

    assert_no_2304("sibling nested infers", &check_source_strict(source));
}

#[test]
fn nested_infer_with_mapped_true_branch_visible() {
    // Exercises the second registration path
    // (`push_infer_bindings_from_extends`) which fires only when the true
    // branch is a MAPPED_TYPE. Both the outer `V` and the nested `Item` must
    // resolve in the mapped template.
    let source = r#"
type WithMapped<T> =
  T extends infer V extends { items: infer Item extends number }
    ? { [K in "x"]: [V, Item] }
    : never;

type R = WithMapped<{ items: 7 }>;
"#;

    assert_no_2304("mapped true branch", &check_source_strict(source));
}

#[test]
fn nested_infer_visible_inside_indexed_access_constraint() {
    let source = r#"
type IndexedAccess<T> =
  T extends infer V extends { items: Array<infer Item extends string>[number] }
    ? Item
    : never;

type R = IndexedAccess<{ items: "a"[] }>;
"#;

    assert_no_2304("indexed access constraint", &check_source_strict(source));
}

#[test]
fn outer_infer_remains_unaffected_when_constraint_has_no_nested_infer() {
    // Negative-shape: a plain `infer V extends C` with no nested infer must
    // still work — verifies the fix did not regress the outer binding.
    let source = r#"
type Simple<T> =
  T extends infer V extends number
    ? V
    : never;

type R = Simple<5>;
"#;

    assert_no_2304("simple infer extends", &check_source_strict(source));
}

#[test]
fn nested_infer_name_unbound_in_false_branch() {
    // The false branch must NOT see the nested name — same scope rule as
    // outer infers. tsc emits TS2304 here.
    let source = r#"
type FalseBranch<T> =
  T extends { value: infer V extends { nested: infer N extends number } }
    ? V
    : N;
"#;

    let diags = check_source_strict(source);
    let n_errors: Vec<_> = ts2304(&diags)
        .into_iter()
        .filter(|d| d.message_text.contains("'N'"))
        .collect();
    assert_eq!(
        n_errors.len(),
        1,
        "expected exactly one TS2304 for `N` in false branch; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
}
