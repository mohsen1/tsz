//! Tests for TS2456 circular-type-alias detection through tuple spread
//! elements.
//!
//! `tsc` defers tuple *element* resolution, so a plain or optional element
//! never makes a tuple alias circular (`type T = [T]`, `type T = [T?]`, and
//! `type T = [number, T[]]` are all accepted). A *spread* element is different:
//! splicing `...X` into the enclosing tuple forces `X` to be resolved to a
//! concrete tuple, re-entering any alias `X` reaches. The single exception is a
//! spread whose operand is written directly as an array type `...Y[]`, which the
//! array fast-path keeps deferred. These tests lock that behavior to match
//! `tsc` 6.0.2, including renamed binders so no name-literal drives the logic.

use tsz_checker::test_utils::check_source_codes as get_error_codes;

fn assert_ts2456(src: &str) {
    let codes = get_error_codes(src);
    assert!(
        codes.contains(&2456),
        "Expected TS2456 (circularly references itself) for:\n{src}\ngot: {codes:?}"
    );
}

fn assert_no_ts2456(src: &str) {
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 for:\n{src}\ngot: {codes:?}"
    );
}

#[test]
fn direct_tuple_self_spread_is_circular() {
    assert_ts2456("type T = [number, ...T];");
}

#[test]
fn bare_tuple_self_spread_is_circular() {
    assert_ts2456("type U = [...U];");
}

#[test]
fn renamed_binder_self_spread_is_circular() {
    // No dependence on a particular alias name.
    assert_ts2456("type Rec = [string, ...Rec];");
    assert_ts2456("type SomethingElse = [boolean, ...SomethingElse];");
}

#[test]
fn named_tuple_member_self_spread_is_circular() {
    assert_ts2456("type Named = [a: number, ...Named];");
}

#[test]
fn readonly_tuple_self_spread_is_circular() {
    assert_ts2456("type ReadonlyRec = readonly [number, ...ReadonlyRec];");
}

#[test]
fn parenthesized_self_spread_is_circular() {
    assert_ts2456("type SelfRef = [...(SelfRef)];");
}

#[test]
fn nested_inline_tuple_self_spread_is_circular() {
    // Splicing an inline tuple forces all of its elements.
    assert_ts2456("type Y = [...[Y]];");
    assert_ts2456("type X = [...[...X]];");
    assert_ts2456("type V = [...[number, V]];");
}

#[test]
fn alias_hop_self_spread_is_circular() {
    // Spreading a bare alias resolves its declared body.
    assert_ts2456("type ViaAlias = [...Alias2];\ntype Alias2 = ViaAlias;");
    assert_ts2456("type Q = [...M];\ntype M = [Q];");
}

#[test]
fn generic_array_self_spread_is_circular() {
    // `Array<T>` / `ReadonlyArray<T>` are not the deferred `T[]` array fast-path,
    // so the type argument is forced.
    assert_ts2456("type T = [...Array<T>];");
    assert_ts2456("type R = [number, ...ReadonlyArray<R>];");
}

#[test]
fn array_wrapped_spread_is_not_circular() {
    // The one deferred spread form: `...Y[]`.
    assert_no_ts2456("type T2 = [number, ...T2[]];");
    assert_no_ts2456("type RO = readonly [number, ...RO[]];");
}

#[test]
fn plain_and_optional_tuple_elements_are_not_circular() {
    assert_no_ts2456("type Self = [Self];");
    assert_no_ts2456("type SelfOpt = [Self2?];\ntype Self2 = [Self2?];");
}

#[test]
fn object_or_function_wrapped_spread_arg_is_not_circular() {
    // A structural wrapper inside the forced operand defers its members.
    assert_no_ts2456("type T = [...Array<{ next: T }>];");
    assert_no_ts2456("type T = [...({ next: T })[]];");
}

#[test]
fn alias_to_tuple_plain_element_is_not_circular() {
    // A plain element behind an alias boundary defers (no forcing into the body).
    assert_no_ts2456("type P = [...[Q]];\ntype Q = [P];");
}

#[test]
fn non_self_spread_tuple_is_not_circular() {
    assert_no_ts2456("type Ok = [number, ...Other];\ntype Other = [string, string];");
}

#[test]
fn control_simple_self_alias_still_circular() {
    // The pre-existing simple self-cycle path is unaffected.
    assert_ts2456("type A = A;");
}
