//! Regression fence: instantiating a tuple type with two or more spread
//! elements whose operands are readonly fixed-length tuples must flatten each
//! operand's elements and concatenate them, not collapse them into a single
//! `...(A | B)[]` rest element.
//!
//! Structural rule: a spread `...X` inside a tuple whose instantiated operand
//! `X` is a fixed-length tuple contributes `X`'s fixed element slots inline,
//! whether or not `X` is `readonly`. tsc drops the container's readonly-ness
//! when spreading fixed elements into a mutable tuple target, keeping only each
//! element's own type (`[...readonly [1, 2], ...readonly ["a"]]` ->
//! `[1, 2, "a"]`). An unbounded `readonly T[]` operand stays a rest element
//! (`[...readonly [1, 2], ...readonly boolean[]]` -> `[1, 2, ...boolean[]]`).
//!
//! Before the fix, tuple instantiation only flattened a rest operand that was a
//! bare (non-`readonly`) fixed tuple, so a `readonly [...]` operand was kept
//! whole as a single rest element; two such adjacent rest elements were then
//! merged into `...(A | B)[]`, producing e.g.
//! `(readonly [1, 2] | readonly ["a"])[]` where tsc yields `[1, 2, "a"]`. The
//! single-operand case was masked because the rest-merge pass no-ops below
//! length 2.
//!
//! Owner layer: solver tuple instantiation.
//!
//! Binder names are varied across cases so the fix is structural, not
//! name-keyed. Every fixture is pinned against the `tsc` 7.0.2 oracle.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts2322_count(source: &str) -> usize {
    compile_and_get_diagnostics(source)
        .iter()
        .filter(|(code, _)| *code == 2322)
        .count()
}

#[test]
fn type_level_two_readonly_tuple_spreads_concatenate() {
    // `Concat<readonly [1, 2], readonly ["a"]>` is `[1, 2, "a"]`, so index 1 is
    // `2` and index 2 is `"a"`. Before the fix the alias resolved to
    // `(readonly [1, 2] | readonly ["a"])[]` and both reads failed.
    let source = r#"
type Concat<A extends readonly unknown[], B extends readonly unknown[]> = [...A, ...B];
type R = Concat<readonly [1, 2], readonly ["a"]>;
const r1: 2 = (null as unknown as R)[1];
const r2: "a" = (null as unknown as R)[2];
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "two readonly-tuple spreads must concatenate to [1, 2, \"a\"]"
    );
}

#[test]
fn inferred_concat_of_two_readonly_tuple_args() {
    // The real-world witness: a generic `concat` whose `[...A, ...B]` return
    // type is instantiated from two `as const` (readonly-tuple) arguments.
    let source = r#"
function concat<A extends readonly unknown[], B extends readonly unknown[]>(a: A, b: B): [...A, ...B] {
  return [] as unknown as [...A, ...B];
}
const c = concat([1, 2] as const, ["a"] as const);
const c1: 2 = c[1];
const c2: "a" = c[2];
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "inferred [...A, ...B] over two readonly tuple args must be [1, 2, \"a\"]"
    );
}

#[test]
fn prefix_element_with_two_readonly_tuple_spreads_renamed_binders() {
    // Prefix element preserved and both readonly-tuple operands flattened:
    // `[0, ...readonly [1], ...readonly [2, 3]]` -> `[0, 1, 2, 3]`. Binder
    // spellings differ from the other cases to keep the fix structural.
    let source = r#"
type Join<Head extends readonly unknown[], Tail extends readonly unknown[]> = [0, ...Head, ...Tail];
type Out = Join<readonly [1], readonly [2, 3]>;
const o0: 0 = (null as unknown as Out)[0];
const o1: 1 = (null as unknown as Out)[1];
const o3: 3 = (null as unknown as Out)[3];
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "prefix element plus two readonly-tuple spreads must be [0, 1, 2, 3]"
    );
}

#[test]
fn three_readonly_tuple_spreads_concatenate() {
    // Three adjacent readonly-tuple spreads all flatten: index 2 is `3`.
    let source = r#"
type C3<A extends readonly unknown[], B extends readonly unknown[], D extends readonly unknown[]> = [...A, ...B, ...D];
type R = C3<readonly [1], readonly [2], readonly [3]>;
const r2: 3 = (null as unknown as R)[2];
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "three readonly-tuple spreads must concatenate to [1, 2, 3]"
    );
}

#[test]
fn readonly_tuple_then_readonly_array_keeps_rest() {
    // A readonly fixed tuple flattens while an unbounded readonly array in the
    // same construct stays a rest element:
    // `[...readonly [1, 2], ...readonly boolean[]]` -> `[1, 2, ...boolean[]]`.
    let source = r#"
type Mix<A extends readonly unknown[], B extends readonly unknown[]> = [...A, ...B];
type R = Mix<readonly [1, 2], readonly boolean[]>;
const r0: 1 = (null as unknown as R)[0];
const r1: 2 = (null as unknown as R)[1];
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "the fixed readonly-tuple head must flatten to [1, 2, ...boolean[]]"
    );
}

#[test]
fn two_readonly_unbounded_arrays_stay_a_union_rest() {
    // Negative control: two unbounded readonly arrays are NOT fixed tuples, so
    // they correctly collapse to `(string | number)[]` (tsc-verified). The
    // element read is therefore `string | number`, which is not assignable to
    // the narrow literal `1` — proving the fix does not over-flatten arrays.
    let source = r#"
type C<A extends readonly unknown[], B extends readonly unknown[]> = [...A, ...B];
type R = C<readonly number[], readonly string[]>;
const bad: 1 = (null as unknown as R)[0];
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "two readonly unbounded arrays must stay (string | number)[], element not assignable to 1"
    );
}
