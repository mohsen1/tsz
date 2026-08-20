//! Regression tests for `keyof T` collapsing to `never` in heritage member
//! checks (`TS2416` / `TS2430`).
//!
//! A base interface member written as `keyof T` is a *deferred* index type
//! until `T` is instantiated — `tsc` keeps it deferred and compares it
//! structurally against the implementing member. The heritage member checks
//! re-derive the base member type through `get_type_of_interface_member_simple`
//! -> `get_keyof_type`, whose last fallback collapsed any operand it could not
//! collect keys from to `never`. For a type-parameter operand there are no keys
//! to collect *yet*, so the base member became `never`: every implementation of
//! it was reported as incompatible (`Type 'keyof DB' is not assignable to type
//! 'never'`), and inside a union the `keyof` arm silently vanished.
//!
//! The decision is structural — "does the operand still mention a type
//! parameter" — so the cases below vary the binder names, the heritage form
//! (`implements` vs interface `extends`), the member form (property, method,
//! accessor), and the nesting (bare, union arm, type argument, through an
//! alias). Concrete operands are unaffected: `keyof {}` is still `never`.

use crate::test_utils::check_source_codes;

#[track_caller]
fn assert_clean(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        !codes.contains(&2416) && !codes.contains(&2430),
        "unexpected heritage member diagnostic (false positive). Got: {codes:?}\nSource:\n{src}"
    );
}

#[track_caller]
fn assert_reports(code: u32, src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.contains(&code),
        "expected TS{code}, got {codes:?}\nSource:\n{src}"
    );
}

#[test]
fn implements_bare_keyof_member_is_clean() {
    assert_clean(
        r"
interface Container<DB> { fn: keyof DB; }
class Impl<DB> implements Container<DB> { fn!: keyof DB; }
",
    );
}

#[test]
fn implements_bare_keyof_member_is_clean_with_renamed_binders() {
    assert_clean(
        r"
interface Registry<Shape> { key: keyof Shape; }
class Store<Shape> implements Registry<Shape> { key!: keyof Shape; }
",
    );
}

#[test]
fn interface_extends_bare_keyof_member_is_clean() {
    assert_clean(
        r"
interface Container<DB> { fn: keyof DB; }
interface Impl<DB> extends Container<DB> { fn: keyof DB; }
",
    );
}

#[test]
fn interface_extends_keyof_member_with_concrete_type_argument_is_clean() {
    assert_clean(
        r"
interface Container<DB> { fn: keyof DB; }
interface Impl extends Container<{ a: 1 }> { fn: 'a'; }
",
    );
}

#[test]
fn keyof_arm_survives_in_a_union_member() {
    assert_clean(
        r"
interface Container<DB> { fn: keyof DB | number; }
interface Impl<DB> extends Container<DB> { fn: keyof DB | number; }
",
    );
}

#[test]
fn keyof_member_with_constrained_type_parameter_is_clean() {
    assert_clean(
        r"
interface Container<DB extends object> { fn: keyof DB; }
interface Impl<DB extends object> extends Container<DB> { fn: keyof DB; }
",
    );
}

#[test]
fn keyof_method_return_member_is_clean() {
    assert_clean(
        r"
interface Container<DB> { m(): keyof DB; }
class Impl<DB> implements Container<DB> { m(): keyof DB { return null!; } }
",
    );
}

#[test]
fn keyof_nested_through_a_generic_alias_is_clean() {
    assert_clean(
        r"
type Wrap<T> = { k: keyof T };
interface Container<DB> { m: Wrap<DB>; }
interface Impl<DB> extends Container<DB> { m: Wrap<DB>; }
",
    );
}

#[test]
fn keyof_as_a_type_argument_under_an_accessor_is_clean() {
    // The shape reduced from the kysely `FunctionModule` false positive: the
    // base member's `keyof DB` sits in a type-argument position and the
    // implementing member is a getter whose body is a zero-argument generic
    // call.
    assert_clean(
        r"
interface FunctionModule<DB, TB extends keyof DB> {
    agg: TB & string;
    plain: TB;
}
declare function createFunctionModule<DB, TB extends keyof DB>(): FunctionModule<DB, TB>;
interface Container<DB> {
    fn: FunctionModule<DB, keyof DB>;
}
class Impl<DB> implements Container<DB> {
    get fn(): FunctionModule<DB, keyof DB> {
        return createFunctionModule();
    }
}
",
    );
}

#[test]
fn genuine_mismatch_against_a_deferred_keyof_is_still_reported() {
    assert_reports(
        2430,
        r"
interface Base<Q> { m: keyof Q; }
interface Derived<Q> extends Base<Q> { m: number; }
",
    );
}

#[test]
fn genuine_mismatch_against_an_instantiated_keyof_is_still_reported() {
    assert_reports(
        2430,
        r"
interface Base<Q> { m: keyof Q; }
interface Derived extends Base<{ a: 1 }> { m: 'b'; }
",
    );
}

#[test]
fn genuine_implements_mismatch_against_a_deferred_keyof_is_still_reported() {
    assert_reports(
        2416,
        r"
interface Base<Q> { m: keyof Q; }
class Derived<Q> implements Base<Q> { m!: number; }
",
    );
}

#[test]
fn keyof_of_a_concrete_empty_object_is_still_never() {
    // The concrete half of the fallback is unchanged: an operand with no type
    // parameters and no collectable keys stays `never`, so a string is not
    // assignable to it.
    assert_reports(
        2322,
        r"
type E = keyof {};
const e: E = 'a';
",
    );
}
