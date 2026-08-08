//! TS2411 checks a property's *materialized* type against an index signature,
//! matching tsc's `getTypeOfSymbol` (a reduced type). A union whose constituent
//! is subsumed by a sibling — a numeric enum inside `E | number`, since every
//! enum value is a `number` — collapses to that sibling before the constraint
//! is judged. Judging the raw, unmaterialized union member-by-member made the
//! absorbed enum constituent spuriously fail `E -> indexType` and emit a false
//! TS2411 (even while the rendered type already showed the reduced form).
//!
//! Regression for `unionSubtypeIfEveryConstituentTypeIsSubtype.ts` (the extra
//! `foo2` TS2411) tracked in #16866; the underlying decision-vs-display
//! asymmetry is the apparent-type materialize-before-decide gateway (#15396).
//! Oracled against pinned `typescript@7.0.2`.
use tsz_checker::test_utils::check_source_diagnostics;

fn ts2411_props(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2411)
        // Pull the offending property name out of the message so assertions read
        // structurally rather than pinning the whole rendered string.
        .map(|d| d.message_text.clone())
        .collect()
}

#[test]
fn numeric_enum_union_with_number_is_absorbed_before_the_index_check() {
    // `E | number` reduces to `number`, which is assignable to a numeric-enum
    // index — so `foo2` must NOT report. `foo` (`string | number`) still reports
    // because `string` is not assignable.
    let props = ts2411_props(
        r#"
enum e { e1, e2 }
enum E2 { A }
interface I14 {
    [x: string]: E2;
    foo: string | number;
    foo2: e | number;
}
"#,
    );
    assert!(
        props.iter().any(|m| m.contains("'foo'")),
        "`foo: string | number` must still report against the enum index; got: {props:?}"
    );
    assert!(
        !props.iter().any(|m| m.contains("'foo2'")),
        "`foo2: e | number` reduces to `number` and must not report; got: {props:?}"
    );
}

#[test]
fn numeric_enum_member_union_with_number_is_absorbed() {
    // The enum *member* form reduces the same way a numeric literal does
    // (`1 | number` -> `number`): a single enum member is a number subtype.
    let props = ts2411_props(
        r#"
enum e { e1, e2 }
enum E2 { A }
interface A2 {
    [x: string]: E2;
    p: e.e1 | number;
}
"#,
    );
    assert!(
        props.is_empty(),
        "`e.e1 | number` reduces to `number`; no TS2411 expected, got: {props:?}"
    );
}

#[test]
fn genuinely_unassignable_union_member_still_reports() {
    // Negative control: `string | boolean` shares no constituent absorbed by the
    // numeric-enum index, so the check must still fire. Guards against the
    // materialize step masking real errors.
    let props = ts2411_props(
        r#"
enum E2 { A }
interface Bad {
    [x: string]: E2;
    q: string | boolean;
}
"#,
    );
    assert!(
        props.iter().any(|m| m.contains("'q'")),
        "a union with no index-assignable-only reduction must still report; got: {props:?}"
    );
}
