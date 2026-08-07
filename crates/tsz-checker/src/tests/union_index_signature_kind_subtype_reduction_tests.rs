//! Regression tests for union subtype reduction dropping a member whose
//! index-signature KEY KIND the surviving sibling lacks (TS2353 false
//! positive, `compiler/unionExcessPropertyCheckNoApparentPropTypeMismatchErrors.ts`).
//!
//! Structural rule: `{ [k: string]: V }` is a structural subtype of
//! `{ [k: number]: V }` (a string index covers numeric keys), but when a
//! declared union carries both, tsc's `isKnownProperty` consults the union as
//! written, so a fresh object literal's string-keyed property is admitted
//! through the string-indexed member. tsz preserves that member through the
//! kind-aware index-signature veto in the solver's compound simplification
//! (`remove_redundant_members` in
//! `crates/tsz-solver/src/evaluation/evaluate/compound_simplification.rs`):
//! a member is not removable when it carries an index-signature key kind
//! (`string` / `number` / `symbol`) the subsuming sibling lacks.
//!
//! Oracle: `typescript@7.0.2` (`--strict false --target es2015`) reports
//! nothing for every "clean" row below.

use crate::test_utils::{check_source_codes, check_source_diagnostics};

#[test]
fn string_index_member_survives_number_index_sibling_in_call_argument() {
    // Witness (reduced from the conformance fixture): the string-keyed
    // property is admitted through the string-indexed union member; the
    // number-indexed sibling must not swallow it via subtype reduction.
    let codes = check_source_codes(
        r#"
interface WordCounts { [word: string]: number; }
interface SlotCounts { [slot: number]: number; }
declare function tally(from: WordCounts | SlotCounts): void;
tally({ total: 123 });
"#,
    );
    assert!(
        !codes.contains(&2353),
        "string-keyed property admitted by the string-indexed union member must not be excess, got: {codes:?}"
    );
}

#[test]
fn union_member_order_does_not_change_the_verdict() {
    // Same shape with the members swapped: removal used to be order-blind, so
    // the fix must be too.
    let codes = check_source_codes(
        r#"
interface WordCounts { [word: string]: number; }
interface SlotCounts { [slot: number]: number; }
declare function tally(from: SlotCounts | WordCounts): void;
tally({ total: 123 });
"#,
    );
    assert!(
        !codes.contains(&2353),
        "member order must not affect index-signature admission, got: {codes:?}"
    );
}

#[test]
fn generic_instantiated_union_admits_string_keyed_property() {
    // The conformance fixture's own shape: generic dictionary interfaces
    // instantiated through call-site inference, including the
    // `Object.prototype`-named key `toString` the fixture uses.
    let codes = check_source_codes(
        r#"
interface IStringDictionary<V> { [name: string]: V; }
interface INumberDictionary<V> { [idx: number]: V; }
declare function forEach<T>(
    from: IStringDictionary<T> | INumberDictionary<T>,
    callback: (entry: { key: any; value: T; }, remove: () => void) => any,
): void;
let count = 0;
forEach({ toString: 123 }, () => count++);
"#,
    );
    assert!(
        !codes.contains(&2353),
        "the conformance fixture's call must not report an excess property, got: {codes:?}"
    );
}

#[test]
fn assignment_position_union_stays_clean() {
    // The assignment path already consulted the declared union; keep it that
    // way.
    let codes = check_source_codes(
        r#"
interface WordCounts { [word: string]: number; }
interface SlotCounts { [slot: number]: number; }
var counts: WordCounts | SlotCounts = { total: 123 };
"#,
    );
    assert!(
        !codes.contains(&2353),
        "assignment to the declared union must stay clean, got: {codes:?}"
    );
}

#[test]
fn value_mismatch_against_the_index_value_type_still_errors() {
    // Negative control: the property name is admitted, but its VALUE violates
    // the index value type — tsc reports TS2322 here (elaborated member
    // mismatch), not silence.
    let diags = check_source_diagnostics(
        r#"
interface WordCounts { [word: string]: number; }
interface SlotCounts { [slot: number]: number; }
declare function tally(from: WordCounts | SlotCounts): void;
tally({ total: "not a number" });
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "a value mismatch against the admitting index signature must still error, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn plain_object_union_still_reports_excess_property() {
    // Positive control: with no index signatures anywhere in the union, an
    // unknown property is still excess (TS2353 must keep firing).
    let codes = check_source_codes(
        r#"
interface Circle { radius: number; }
interface Square { side: number; }
declare function area(shape: Circle | Square): void;
area({ radius: 1, bogus: 2 });
"#,
    );
    assert!(
        codes.contains(&2353),
        "a genuinely unknown property against a plain object union must stay excess, got: {codes:?}"
    );
}

#[test]
fn same_kind_index_members_still_reduce_without_regression() {
    // Two string-indexed members where one genuinely subsumes the other:
    // reduction (or not) must never manufacture an excess-property report,
    // since both members admit every string key.
    let codes = check_source_codes(
        r#"
interface Narrow { [k: string]: number; }
interface Wide { [k: string]: number | string; }
declare function eat(from: Narrow | Wide): void;
eat({ anything: 5 });
"#,
    );
    assert!(
        !codes.contains(&2353),
        "string-keyed property against string-indexed members is never excess, got: {codes:?}"
    );
}
