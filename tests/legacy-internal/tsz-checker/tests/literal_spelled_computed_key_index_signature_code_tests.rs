//! A computed key spelled with a *literal* is an ordinary property name, so it
//! never takes the computed-property diagnostic TS2418 — not even when the
//! target matches it only through an index signature.
//!
//! Structural rule, oracled against `typescript@7.0.2`: tsc's
//! `isComputedNonLiteralName` is **false** for a computed name written as a
//! string, numeric, or no-substitution-template literal (`["p"]`, `[0]`,
//! `` [`p`] ``). Such a name is not late-bound at all, so a value mismatch
//! under it reports `TS2322` anchored at the value and an excess property
//! reports `TS2353`, exactly as the plainly written `{ p: ... }` does. Only a
//! *late-bound* spelling — an identifier over a `const`, an enum member, or a
//! symbol reference — reaches
//! `Type of computed property's value is '{0}', which is not assignable to
//! type '{1}'` (TS2418).
//!
//! tsz applied TS2418 to every computed key that matched the target through an
//! index signature, regardless of spelling. The existing literal-key exemption
//! was conditioned on the target having a *named* member for the key, so it
//! covered `{ member: number } = { ["member"]: "text" }` and missed every
//! index-signature target. Both the contextual index-signature reporter and
//! the call-argument elaborator carried the same gap, which is why the rows
//! below are paired across an initializer and a call argument.
//!
//! The negative controls are the load-bearing half: each literal spelling is
//! paired with the late-bound spelling of the *same* target, so a later
//! widening of this exemption cannot silently capture `[label]`, `[E.A]`,
//! `[sym]`, or `[Symbol.iterator]`. Binder names are varied throughout so no
//! identifier string is load-bearing.

use crate::test_utils::check_source_strict_messages;

fn codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source_strict_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    codes.sort_unstable();
    codes
}

fn message_for(source: &str, code: u32) -> Option<String> {
    check_source_strict_messages(source)
        .into_iter()
        .find(|(c, _)| *c == code)
        .map(|(_, message)| message)
}

// ---------------------------------------------------------------------------
// Literal-spelled computed keys against an index-signature target: TS2322.
// ---------------------------------------------------------------------------

#[test]
fn a_string_literal_computed_key_against_a_string_index_reports_ts2322() {
    let source = r#"
interface Registry { [entry: string]: string }
const listing: Registry = { ["alpha"]: 1 };
"#;
    assert_eq!(codes(source), vec![2322]);
    let message = message_for(source, 2322).expect("TS2322 for the value mismatch");
    assert!(
        message.contains("'number'") && message.contains("'string'"),
        "the ordinary assignability message must name value and target, got: {message}"
    );
}

#[test]
fn a_numeric_literal_computed_key_against_a_number_index_reports_ts2322() {
    let source = r#"
interface Slots { [slot: number]: string }
const filled: Slots = { [0]: 1 };
"#;
    assert_eq!(codes(source), vec![2322]);
}

#[test]
fn a_no_substitution_template_computed_key_against_a_string_index_reports_ts2322() {
    let source = r#"
interface Catalog { [item: string]: string }
const stocked: Catalog = { [`beta`]: 1 };
"#;
    assert_eq!(codes(source), vec![2322]);
}

#[test]
fn a_string_literal_computed_key_against_a_symbol_and_string_index_reports_ts2322() {
    // The target carries both index signatures; the string one owns a
    // literal-spelled key, so the symbol half must not pull it into TS2418.
    let source = r#"
interface Mixed { [text: string]: string; [tag: symbol]: string }
const blended: Mixed = { ["gamma"]: 1 };
"#;
    assert_eq!(codes(source), vec![2322]);
}

#[test]
fn a_literal_spelled_key_in_a_call_argument_reports_ts2322() {
    // The call-argument elaborator is a second owner of the same decision.
    let source = r#"
interface Ledger { [row: string]: string }
declare function post(book: Ledger): void;
post({ ["delta"]: 1 });
"#;
    assert_eq!(codes(source), vec![2322]);
}

#[test]
fn a_numeric_literal_spelled_key_in_a_call_argument_reports_ts2322() {
    let source = r#"
interface Rows { [row: number]: string }
declare function submit(sheet: Rows): void;
submit({ [0]: 1 });
"#;
    assert_eq!(codes(source), vec![2322]);
}

#[test]
fn a_literal_spelled_key_with_an_object_value_anchors_the_nested_mismatch() {
    // Falling out of the computed branch hands the member to the ordinary
    // elaborator, which descends into an elaboratable value and reports the
    // leaf — `string` -> `number` at `depth`, not an aggregate at the key.
    let source = r#"
interface Shelf { [slot: string]: { depth: number } }
const stacked: Shelf = { ["epsilon"]: { depth: "deep" } };
"#;
    assert_eq!(codes(source), vec![2322]);
    let message = message_for(source, 2322).expect("TS2322 for the nested mismatch");
    assert!(
        message.contains("'string'") && message.contains("'number'"),
        "the leaf mismatch must be reported, not the aggregate object type, got: {message}"
    );
    assert!(
        !message.contains("depth:"),
        "an aggregate object-type message means the nested elaboration did not run, got: {message}"
    );
}

#[test]
fn a_literal_spelled_key_with_an_excess_nested_property_reports_ts2353() {
    let source = r#"
interface Bay { [slot: string]: { width: number } }
const parked: Bay = { ["zeta"]: { width: 1, height: 2 } };
"#;
    assert_eq!(codes(source), vec![2353]);
}

#[test]
fn one_object_literal_can_mix_both_spellings_and_gets_both_codes() {
    // The decision is per-member, not per-literal.
    let source = r#"
const marker = "eta";
interface Book { [page: string]: string }
const opened: Book = { ["theta"]: 1, [marker]: 2 };
"#;
    assert_eq!(codes(source), vec![2322, 2418]);
}

// ---------------------------------------------------------------------------
// Negative controls: every late-bound spelling keeps TS2418.
// ---------------------------------------------------------------------------

#[test]
fn a_late_bound_const_string_key_against_an_index_target_keeps_ts2418() {
    let source = r#"
const heading = "iota";
interface Index { [term: string]: string }
const built: Index = { [heading]: 1 };
"#;
    assert_eq!(codes(source), vec![2418]);
}

#[test]
fn a_late_bound_const_number_key_against_an_index_target_keeps_ts2418() {
    let source = r#"
const position = 0;
interface Grid { [cell: number]: string }
const drawn: Grid = { [position]: 1 };
"#;
    assert_eq!(codes(source), vec![2418]);
}

#[test]
fn an_enum_member_key_against_an_index_target_keeps_ts2418() {
    let source = r#"
enum Channel { Primary = "primary" }
interface Feed { [name: string]: string }
const wired: Feed = { [Channel.Primary]: 1 };
"#;
    assert_eq!(codes(source), vec![2418]);
}

#[test]
fn a_unique_symbol_key_against_a_symbol_index_target_keeps_ts2418() {
    let source = r#"
declare const token: unique symbol;
interface Vault { [held: symbol]: string }
const sealed: Vault = { [token]: 1 };
"#;
    assert_eq!(codes(source), vec![2418]);
}

// A well-known symbol key (`[Symbol.iterator]`) is the fourth late-bound
// spelling and is *not* pinned here: this harness compiles without the es2015
// lib, so `Symbol` does not resolve and the row would assert TS2583 rather than
// the code under test. It is covered by the enum-member row above, which
// exercises the same property-access spelling through the same predicate, and
// was verified directly against the pinned `typescript@7.0.2` — both report
// TS2418 for `{ [held: symbol]: string } = { [Symbol.iterator]: 1 }`.

#[test]
fn a_late_bound_key_in_a_call_argument_keeps_ts2418() {
    let source = r#"
const column = "kappa";
interface Table { [field: string]: string }
declare function insert(into: Table): void;
insert({ [column]: 1 });
"#;
    assert_eq!(codes(source), vec![2418]);
}

// ---------------------------------------------------------------------------
// Named-member targets are unchanged: this fix only widens the index-signature
// half of the existing literal-key exemption.
// ---------------------------------------------------------------------------

#[test]
fn a_literal_spelled_key_against_a_named_member_still_reports_ts2322() {
    let source = r#"
type Record = { lambda: number };
const kept: Record = { ["lambda"]: "text" };
"#;
    assert_eq!(codes(source), vec![2322]);
}

#[test]
fn a_late_bound_key_against_a_named_member_still_reports_ts2418() {
    let source = r#"
const field = "mu";
type Holder = { mu: number };
const stored: Holder = { [field]: "text" };
"#;
    assert_eq!(codes(source), vec![2418]);
}

#[test]
fn a_matching_literal_spelled_key_against_an_index_target_is_clean() {
    // The non-error direction: the exemption must not manufacture a
    // diagnostic where the value is assignable.
    let source = r#"
interface Store { [key: string]: string }
const saved: Store = { ["nu"]: "ok", [`xi`]: "ok", [0]: "ok" };
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}
