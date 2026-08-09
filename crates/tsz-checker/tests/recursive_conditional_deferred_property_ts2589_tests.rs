//! A recursive conditional alias whose recursive branch grows an argument but
//! places the recursive call in a *deferred* position — an object property, a
//! function signature, or a mapped template — must NOT report TS2589.
//!
//! Structural rule: at a use site, tsz's convergence check treats a residual
//! self-application whose argument weight grew as evidence of infinite
//! instantiation. That is only sound when the residual sits at a position `tsc`
//! eagerly expands (a tuple/array element, a union/intersection member, an
//! indexed-access/`keyof` operand, a resolved conditional branch, or an
//! application argument). A residual `tsc` *defers* — the value of an object
//! property, a function parameter/return, or a mapped template — is how `tsc`
//! ties a finite knot for recursive object/function/mapped types: the alias
//! resolves to a concrete wrapper with the recursive call deferred and reports
//! no error at the definition/use site whether or not the recursion is bounded.
//!
//! Witness: issue #17028 — `type Nest<N extends unknown[]> = N["length"] extends
//! 60 ? number : { a: Nest<[unknown, ...N]> }` tripped tsz's depth guard at a
//! nesting level `tsc` completes. Eager growth (tuple/union) stays flagged.
//!
//! Binder names are varied so no name literal drives the logic.

use tsz_checker::test_utils::check_source_codes as get_error_codes;

fn assert_no_ts2589(src: &str) {
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2589),
        "Expected no TS2589 (deferred-position recursion) for:\n{src}\ngot: {codes:?}"
    );
}

fn assert_ts2589(src: &str) {
    let codes = get_error_codes(src);
    assert!(
        codes.contains(&2589),
        "Expected TS2589 (eager divergent recursion) for:\n{src}\ngot: {codes:?}"
    );
}

#[test]
fn non_tail_object_property_counter_recursion_is_not_ts2589() {
    // The reported repro: growth deferred behind an object property, bounded by
    // a `["length"] extends 60` base case `tsc` completes.
    assert_no_ts2589(
        r#"type Nest<N extends unknown[]> = N["length"] extends 60 ? number : { a: Nest<[unknown, ...N]> };
           type Z = Nest<[]>;"#,
    );
}

#[test]
fn renamed_binder_object_property_recursion_is_not_ts2589() {
    // No dependence on a particular alias, parameter, or property name.
    assert_no_ts2589(
        r#"type Loop<Acc extends unknown[]> = Acc["length"] extends 40 ? boolean : { next: Loop<[unknown, ...Acc]> };
           type Out = Loop<[]>;"#,
    );
}

#[test]
fn effectively_unbounded_object_property_recursion_is_not_ts2589() {
    // Even with a base case tsz cannot reach, an object-property-deferred
    // recursion is not a definition/use-site error — `tsc` defers it too.
    assert_no_ts2589(
        r#"type BadObj<N extends unknown[]> = N["length"] extends 999999 ? number : { a: BadObj<[unknown, ...N]> };
           type Z = BadObj<[]>;"#,
    );
}

#[test]
fn function_return_deferred_recursion_is_not_ts2589() {
    // A function return position defers the recursive call the same way.
    assert_no_ts2589(
        r#"type Fn<N extends unknown[]> = N["length"] extends 30 ? number : () => Fn<[unknown, ...N]>;
           type Z = Fn<[]>;"#,
    );
}

#[test]
fn eager_tuple_spread_growth_still_reports_ts2589() {
    // Control: the same counter recursion with the recursive call in an eager
    // tuple-spread position stays flagged — `tsc` reports TS2589 there.
    assert_ts2589(
        r#"type Grow<N extends unknown[]> = N["length"] extends 999999 ? number : [unknown, ...Grow<[unknown, ...N]>];
           type Z = Grow<[]>;"#,
    );
}

#[test]
fn eager_union_growth_still_reports_ts2589() {
    // Control: growth reached through an eager union member stays flagged.
    assert_ts2589(
        r#"type UnionGrow<N extends unknown[]> = N["length"] extends 999999 ? number : "x" | UnionGrow<[unknown, ...N]>;
           type Z = UnionGrow<[]>;"#,
    );
}
