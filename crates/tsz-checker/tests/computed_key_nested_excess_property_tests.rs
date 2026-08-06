//! TS2353 for an excess property nested inside a computed-key member's value.
//!
//! `check_nested_object_literal_excess_properties`
//! (`crates/tsz-checker/src/state/state_checking/property/excess_property_tail.rs`)
//! matches each outer object-literal element against the property name it is
//! recursing on, but read that name with `get_property_name` — which
//! deliberately declines a computed key backed by an identifier expression
//! (`[door]` for `const door = "room"`), per its own doc comment: callers
//! needing type-based resolution should use `get_property_name_resolved`
//! instead. Because the match never succeeded, the recursive excess-property
//! check into the computed member's own value never ran, so an excess
//! property nested inside it went unreported. tsc's `elaborateElementwise`
//! draws no such distinction — a computed key that resolves to a known
//! member is checked the same as any other. Fixed by resolving the element's
//! name through `get_property_name_resolved`, matching how the *source*
//! object-literal type itself already resolves computed keys
//! (`object_literal/computation.rs`).
//!
//! Oracle: `typescript@7.0.2`, `--noEmit --strict --pretty false --target
//! es2022 --lib es2022`.

use tsz_checker::test_utils::check_source_strict_messages;

fn ts2353_messages(source: &str) -> Vec<String> {
    check_source_strict_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2353)
        .map(|(_, message)| message)
        .collect()
}

/// The issue's exact repro: a `const`-backed computed key's own value carries
/// an excess property. tsc: `TS2353` on `extra`, quoting the resolved member
/// type `'{ size: number; }'`.
#[test]
fn computed_const_key_nested_object_literal_reports_excess_property() {
    let messages = ts2353_messages(
        r#"
const door = "room";
type House = { room: { size: number } };
const built: House = { [door]: { size: 1, extra: 2 } };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2353: {messages:?}");
    assert!(
        messages[0].contains("'extra'") && messages[0].contains("'{ size: number; }'"),
        "expected an excess-property message naming 'extra' against '{{ size: number; }}': {}",
        messages[0]
    );
}

/// Anti-hardcoding: the recursive check keys on the resolved member name
/// matching a real target property, not on the identifiers `door`/`House`/
/// `room`/`size`/`extra`.
#[test]
fn computed_const_key_nested_excess_property_is_binder_name_independent() {
    let messages = ts2353_messages(
        r#"
const key = "cabin";
type Ship = { cabin: { berths: number } };
const vessel: Ship = { [key]: { berths: 2, portholes: 1 } };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2353: {messages:?}");
    assert!(
        messages[0].contains("'portholes'") && messages[0].contains("'{ berths: number; }'"),
        "expected an excess-property message naming 'portholes' against '{{ berths: number; }}': {}",
        messages[0]
    );
}

/// Negative control: the same computed-key shape with no excess property must
/// stay clean — the fix must not turn every computed-key member into a false
/// positive.
#[test]
fn computed_const_key_nested_object_literal_without_excess_property_is_clean() {
    let messages = ts2353_messages(
        r#"
const door = "room";
type House = { room: { size: number } };
const built: House = { [door]: { size: 1 } };
"#,
    );
    assert!(
        messages.is_empty(),
        "no excess property present, expected no TS2353: {messages:?}"
    );
}

/// A syntactically literal computed key (`["room"]`, no `const` indirection)
/// already resolved through `get_property_name`'s own literal-name path
/// before this fix; pinned as a control so the fix's `get_property_name_resolved`
/// switch is not mistaken for what made this row work.
#[test]
fn syntactic_literal_computed_key_nested_excess_property_still_reports() {
    let messages = ts2353_messages(
        r#"
type House = { room: { size: number } };
const built: House = { ["room"]: { size: 1, extra: 2 } };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2353: {messages:?}");
    assert!(
        messages[0].contains("'extra'"),
        "expected an excess-property message naming 'extra': {}",
        messages[0]
    );
}

/// A numeric `const` computed key resolves through the same
/// `get_property_name_resolved` path and must recurse identically.
#[test]
fn computed_numeric_const_key_nested_excess_property_reports() {
    let messages = ts2353_messages(
        r#"
const slot = 1;
type Rack = { 1: { load: number } };
const built: Rack = { [slot]: { load: 1, extra: 2 } };
"#,
    );
    assert_eq!(messages.len(), 1, "exactly one TS2353: {messages:?}");
    assert!(
        messages[0].contains("'extra'"),
        "expected an excess-property message naming 'extra': {}",
        messages[0]
    );
}

/// Negative control: a computed key backed by a non-`const` (mutable `let`)
/// binding has no literal type, so the object literal never resolves a
/// `room` member at all — `tsc` reports `TS2741` for the whole literal
/// missing `room`, not `TS2353` on the nested nonexistent member. Confirms
/// the fix's `get_property_name_resolved` switch does not widen matching
/// beyond `tsc`'s own literal-type requirement.
#[test]
fn computed_non_const_key_nested_object_literal_reports_missing_property_not_excess() {
    let messages = check_source_strict_messages(
        r#"
let door = "room";
type House = { room: { size: number } };
const built: House = { [door]: { size: 1, extra: 2 } };
"#,
    );
    let ts2353: Vec<_> = messages.iter().filter(|(code, _)| *code == 2353).collect();
    assert!(
        ts2353.is_empty(),
        "no member resolves, so no excess-property check should run against it: {ts2353:?}"
    );
    assert!(
        messages.iter().any(|(code, m)| *code == 2741
            && m.contains("'room'")
            && m.contains("but required in type 'House'")),
        "expected TS2741 for the whole literal missing 'room': {messages:?}"
    );
}
