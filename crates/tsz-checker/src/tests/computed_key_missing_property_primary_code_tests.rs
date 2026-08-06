//! #16443 item 2 — an object-literal member written with a computed key keeps
//! the *missing-property* primary code instead of collapsing to TS2418.
//!
//! Structural rule, oracled against `typescript@7.0.2`: when an object-literal
//! property whose name is a computed key resolves to a named member of the
//! target, and that member's own relation failure is a **missing-property**
//! failure, `tsc` reports the missing-property code (TS2741 / TS2739) exactly
//! as it does for a plainly written key. Only a failure that is *not*
//! missing-property falls back to `Type of computed property's value is '{0}',
//! which is not assignable to type '{1}'` (TS2418). tsz previously reported
//! TS2418 for every non-syntactically-literal computed key, so
//! `const rc: Comp = { [K]: { cp: 1 } }` lost the TS2741 that the identical
//! `{ inner: { cp: 1 } }` and `{ ["inner"]: { cp: 1 } }` both produce.
//!
//! The discriminator is the **shape of the relation failure, never the kind of
//! the key**. The oracle agrees across every key kind that reaches this site —
//! a late-bound `const` string, a numeric `const`, an enum member, and a
//! `unique symbol` — which is why the fix asks the relation gateway for its
//! `RelationFailure` rather than inspecting the key expression.
//!
//! The second rule pinned here is the source display inside TS2418 itself.
//! `tsc` widens a fresh literal value unless the *target member type* is
//! literal-bearing — ordinary object-literal freshness, asked of the target
//! rather than of the key. So `{ [S]: 2 }` against `{ [S]: 1 }` keeps `'2'`
//! while `{ [S]: "s" }` against `{ [S]: number }` widens to `'string'`, and a
//! late-bound string key behaves identically. tsz keyed that choice on whether
//! the key named a member at all, so it printed the unwidened `'"s"'`.
//!
//! Binder names are varied throughout so no identifier string is load-bearing,
//! and the negative controls (a union-typed target member, a literal-typed
//! target member, a syntactically literal key) pin the shapes that must keep
//! their current code and display.

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
// Positive rows: the missing-property code survives a computed key.
// ---------------------------------------------------------------------------

#[test]
fn late_bound_const_string_key_keeps_ts2741_for_a_single_missing_property() {
    let source = r#"
const holder = "nested";
type Wrapper = { nested: { alpha: number; beta: number } };
const value: Wrapper = { [holder]: { alpha: 1 } };
"#;
    assert_eq!(codes(source), vec![2741]);
    let message = message_for(source, 2741).expect("TS2741 for the missing member");
    assert!(
        message.contains("'beta'"),
        "TS2741 must name the missing member, got: {message}"
    );
}

#[test]
fn late_bound_const_string_key_keeps_ts2739_for_several_missing_properties() {
    let source = r#"
const pick = "slot";
type Holder = { slot: { first: number; second: number } };
const built: Holder = { [pick]: {} };
"#;
    assert_eq!(codes(source), vec![2739]);
}

#[test]
fn a_numeric_const_key_keeps_the_missing_property_code() {
    let source = r#"
const index = 0;
type Slots = { 0: { lead: number; trail: number } };
const filled: Slots = { [index]: { lead: 1 } };
"#;
    assert_eq!(codes(source), vec![2741]);
}

#[test]
fn an_enum_member_key_keeps_the_missing_property_code() {
    let source = r#"
enum Names { Chosen = "chosen" }
type Bag = { chosen: { one: number; two: number } };
const packed: Bag = { [Names.Chosen]: { one: 1 } };
"#;
    assert_eq!(codes(source), vec![2741]);
}

#[test]
fn a_unique_symbol_key_keeps_the_missing_property_code() {
    let source = r#"
declare const marker: unique symbol;
type Marked = { [marker]: { left: number; right: number } };
const tagged: Marked = { [marker]: { left: 1 } };
"#;
    assert_eq!(codes(source), vec![2741]);
}

#[test]
fn a_non_elaboratable_value_still_reports_the_missing_property_code() {
    // The value is a bare reference, so there is no nested object literal to
    // descend into. tsc still reports the missing-property code, which is why
    // the fix keys on the relation failure rather than on whether the value
    // expression happens to be elaboratable.
    let source = r#"
const route = "carried";
type Carrier = { carried: { near: number; far: number } };
declare const supplied: {};
const moved: Carrier = { [route]: supplied };
"#;
    assert_eq!(codes(source), vec![2739]);
}

#[test]
fn renamed_binders_do_not_change_the_outcome() {
    let source = r#"
const zzz_key = "qqq_member";
type Zzz_Target = { qqq_member: { mmm: number; nnn: number } };
const zzz_value: Zzz_Target = { [zzz_key]: { mmm: 1 } };
"#;
    assert_eq!(codes(source), vec![2741]);
    let message = message_for(source, 2741).expect("TS2741 for the missing member");
    assert!(
        message.contains("'nnn'"),
        "TS2741 must name the missing member, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// Negative controls: shapes that must keep TS2418.
// ---------------------------------------------------------------------------

#[test]
fn a_union_typed_target_member_keeps_ts2418() {
    // The failure is not a missing-property failure, so the computed-property
    // message stands. This is the control that stops the fix from widening
    // into "every computed key takes the named path".
    let source = r#"
const gate = "either";
type Choice = { either: { only: number } | number };
const chosen: Choice = { [gate]: {} };
"#;
    assert_eq!(codes(source), vec![2418]);
}

#[test]
fn a_literal_typed_target_member_keeps_ts2418_and_its_unwidened_source() {
    // Guards the display half: the target member is literal-bearing, so the
    // fresh literal value must NOT widen.
    let source = r#"
declare const stamp: unique symbol;
type Stamped = { [stamp]: 1 };
const applied: Stamped = { [stamp]: 2 };
"#;
    assert_eq!(codes(source), vec![2418]);
    let message = message_for(source, 2418).expect("TS2418 for the literal mismatch");
    assert!(
        message.contains("'2'") && message.contains("'1'"),
        "the literal source must be preserved against a literal target, got: {message}"
    );
}

#[test]
fn a_syntactically_literal_computed_key_still_uses_the_plain_assignability_code() {
    // `["member"]` is a property name, not a late-bound key: tsc reports
    // TS2322 here, never TS2418. Unchanged by this fix, pinned so a later
    // widening of the computed branch cannot silently capture it.
    let source = r#"
type Plain = { member: number };
const written: Plain = { ["member"]: "text" };
"#;
    assert_eq!(codes(source), vec![2322]);
}

// ---------------------------------------------------------------------------
// The TS2418 source display widens against a non-literal target member.
// ---------------------------------------------------------------------------

#[test]
fn a_late_bound_string_key_widens_its_source_against_a_non_literal_target() {
    let source = r#"
const label = "count";
type Counter = { count: number };
const tally: Counter = { [label]: "text" };
"#;
    assert_eq!(codes(source), vec![2418]);
    let message = message_for(source, 2418).expect("TS2418 for the value mismatch");
    assert!(
        message.contains("'string'") && !message.contains("'\"text\"'"),
        "a fresh literal must widen against a non-literal target, got: {message}"
    );
}

#[test]
fn a_unique_symbol_key_widens_its_source_against_a_non_literal_target() {
    let source = r#"
declare const badge: unique symbol;
type Badged = { [badge]: number };
const worn: Badged = { [badge]: "text" };
"#;
    assert_eq!(codes(source), vec![2418]);
    let message = message_for(source, 2418).expect("TS2418 for the value mismatch");
    assert!(
        message.contains("'string'") && !message.contains("'\"text\"'"),
        "a fresh literal must widen against a non-literal target, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// Formerly a documented gap, now closed — flipped deliberately, as intended.
// ---------------------------------------------------------------------------

#[test]
fn excess_property_inside_a_computed_members_value_is_reported() {
    // This row was pinned as `codes(source) == []` when the missing-property
    // fix landed, with the note that whoever closed the excess-property half
    // had to flip it deliberately rather than discover it. Flipped here: the
    // excess check now descends into a computed member's object-literal value,
    // so tsz reports the TS2353 that tsc has always reported for `extra`.
    let source = r#"
const door = "room";
type House = { room: { size: number } };
const built: House = { [door]: { size: 1, extra: 2 } };
"#;
    assert_eq!(codes(source), vec![2353]);
}
