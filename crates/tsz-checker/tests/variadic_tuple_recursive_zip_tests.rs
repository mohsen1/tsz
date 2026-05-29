//! Regression coverage for recursive variadic tuple inference.
//!
//! Structural rule: when a recursive conditional peels two concrete tuples with
//! `[infer Head, ...infer Tail]` and prepends a paired head onto the recursively
//! inferred tail, tuple arity and element positions must remain exact.

use tsz_checker::test_utils::check_source_codes;

fn assert_only_bad_assignment_fails(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "{label}: expected exactly one TS2322 from the deliberate bad leaf, got {codes:?}"
    );
}

#[test]
fn recursive_zip_preserves_nested_variadic_tail_positions() {
    assert_only_bad_assignment_fails(
        r#"
type Prepend_28<T, A extends readonly any[]> = [T, ...A];
type Zip_28<A extends readonly any[], B extends readonly any[]> =
  A extends [infer AH, ...infer AT]
    ? B extends [infer BH, ...infer BT]
      ? [Prepend_28<[AH, BH], Zip_28<AT, BT>>]
      : []
    : [];

type Z_28 = Zip_28<[1, 2, 3], [string, boolean, string]>;
declare const zipped: Z_28;
const shape: [[[1, string], [[2, boolean], [[3, string]]]]] = zipped;
const ok: Z_28 = [[[1, "a"], [[2, true], [[3, "b"]]]]];
const bad: Z_28 = [[[1, "a"], [[2, true], [[3, 123]]]]];
"#,
        "reported Zip_28 tuple composition",
    );
}

#[test]
fn renamed_pair_chain_preserves_variadic_tail_positions() {
    assert_only_bad_assignment_fails(
        r#"
type AddFirst<Item, Rest extends readonly unknown[]> = [Item, ...Rest];
type PairChain<Left extends readonly unknown[], Right extends readonly unknown[]> =
  Left extends [infer FirstLeft, ...infer RemainingLeft]
    ? Right extends [infer FirstRight, ...infer RemainingRight]
      ? [AddFirst<{ left: FirstLeft; right: FirstRight }, PairChain<RemainingLeft, RemainingRight>>]
      : []
    : [];

type Result = PairChain<["x", "y"], [10, 20]>;
declare const result: Result;
const shape: [[{ left: "x"; right: 10 }, [{ left: "y"; right: 20 }]]] = result;
const ok: Result = [[{ left: "x", right: 10 }, [{ left: "y", right: 20 }]]];
const bad: Result = [[{ left: "x", right: 10 }, [{ left: "y", right: 99 }]]];
"#,
        "renamed PairChain tuple composition",
    );
}
