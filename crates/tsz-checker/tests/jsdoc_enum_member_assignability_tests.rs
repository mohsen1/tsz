//! Per-member assignability for JSDoc `@enum {T}`.
//!
//! Issue #9761: tsz emitted one whole-object TS2322 at the variable name
//! when an `@enum {T}` initializer contained a member whose value wasn't
//! assignable to `T`. tsc instead reports TS2322 per offending member,
//! anchored at the property name, with the offending value's type vs `T`.
//!
//! These tests lock the corrected per-member elaboration shape.

use tsz_checker::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source;

fn js_check_options() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..Default::default()
    }
}

fn type_is_not_assignable_to_type(source: &str, target: &str) -> String {
    format!("Type '{source}' is not assignable to type '{target}'.")
}

#[test]
fn reported_repro_anchors_at_property_name_with_value_type() {
    // Issue repro: `@enum {number}` with a string member must produce
    // `Type 'string' is not assignable to type 'number'.` anchored at the
    // member's *name* (matching tsc's `elaborateElementwise`), not one
    // whole-object diagnostic at the variable name.
    let source = "/** @enum {number} */\nconst E = { A: 0, B: \"wrong\" };\n";
    let diagnostics = check_source(source, "repro.js", js_check_options());
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322 (per-member), got: {diagnostics:#?}"
    );
    let diag = ts2322[0];
    assert_eq!(
        diag.message_text,
        type_is_not_assignable_to_type("string", "number"),
        "message should compare value type to enum element type",
    );
    // The anchor must point at the offending member's name (`B`), not at
    // the variable declaration name (`E`). The string `B:` first occurs at
    // byte offset 39 in the source above; that is the property-name span.
    let expected_start = source.find("B: \"wrong\"").expect("test source malformed") as u32;
    assert_eq!(
        diag.start, expected_start,
        "diagnostic must anchor at the offending property name, not the variable name",
    );
}

#[test]
fn symmetric_enum_string_with_numeric_member() {
    // `@enum {string}` with a numeric member: same rule with source/target
    // swapped. Locks that the fix isn't number-typed.
    let source = "/** @enum {string} */\nconst E = { A: \"a\", B: 42 };\n";
    let diagnostics = check_source(source, "sym.js", js_check_options());
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected one TS2322, got: {diagnostics:#?}"
    );
    assert_eq!(
        ts2322[0].message_text,
        type_is_not_assignable_to_type("number", "string"),
    );
    let expected_start = source.find("B: 42").expect("test source malformed") as u32;
    assert_eq!(ts2322[0].start, expected_start);
}

#[test]
fn multiple_offending_members_each_report_independently() {
    // Adjacent case: multiple wrong members each get their own anchored
    // diagnostic at the property name. Locks per-member elaboration,
    // not a single aggregated whole-object error.
    let source = "/** @enum {number} */\nconst Foo = { Bar: 1, Baz: \"no\", Qux: true };\n";
    let diagnostics = check_source(source, "multi.js", js_check_options());
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        2,
        "expected two TS2322 (one per bad member), got: {diagnostics:#?}"
    );
    let baz_pos = source.find("Baz: \"no\"").expect("test source malformed") as u32;
    let qux_pos = source.find("Qux: true").expect("test source malformed") as u32;
    let mut starts: Vec<u32> = ts2322.iter().map(|d| d.start).collect();
    starts.sort_unstable();
    assert_eq!(starts, vec![baz_pos, qux_pos]);
    for diag in &ts2322 {
        assert!(
            diag.message_text == type_is_not_assignable_to_type("string", "number")
                || diag.message_text == type_is_not_assignable_to_type("boolean", "number"),
            "unexpected per-member message: {:?}",
            diag.message_text
        );
    }
}

#[test]
fn renamed_enum_and_members_unchanged() {
    // Anti-hardcoding guard: the structural rule is about the @enum
    // annotation, not any specific name. Renaming the enum and member
    // identifiers must not change the diagnostic shape.
    let source = "/** @enum {number} */\nconst PaletteSlot = { Primary: 0, Accent: \"bad\" };\n";
    let diagnostics = check_source(source, "renamed.js", js_check_options());
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(ts2322.len(), 1, "got: {diagnostics:#?}");
    assert_eq!(
        ts2322[0].message_text,
        type_is_not_assignable_to_type("string", "number"),
    );
    let expected_start = source
        .find("Accent: \"bad\"")
        .expect("test source malformed") as u32;
    assert_eq!(ts2322[0].start, expected_start);
}

#[test]
fn all_matching_members_produce_no_error() {
    // Negative control: a fully-conforming `@enum` must not emit.
    let source = "/** @enum {number} */\nconst Ok = { A: 1, B: 2, C: 3 };\n";
    let diagnostics = check_source(source, "ok.js", js_check_options());
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "expected no TS2322, got: {diagnostics:#?}"
    );
}

#[test]
fn object_freeze_wrapper_does_not_per_member_validate() {
    // tsc's per-member elaboration only fires for *direct* object-literal
    // initializers. `Object.freeze({...})` opts out: the value type stays
    // as the inferred object literal type; any downstream `T`-typed use
    // produces its own diagnostic at the use site. tsz now matches —
    // previously the inner literal was unwrapped and per-member-checked.
    let source = "/** @enum {number} */\nconst F = Object.freeze({ A: 0, B: \"wrong\" });\n";
    let diagnostics = check_source(source, "freeze.js", js_check_options());
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Object.freeze-wrapped @enum should not produce per-member TS2322, got: {diagnostics:#?}"
    );
}

#[test]
fn member_value_assignable_to_union_element_type() {
    // Adjacent case: `@enum {string | number}` with a boolean member
    // still produces a per-member TS2322. The diagnostic anchor and the
    // source-type slot match tsc; the target-type display (the alias name
    // vs the expanded union) is a separate concern tracked elsewhere.
    let source = "/** @enum {string | number} */\nconst U = { A: 1, B: \"ok\", C: true };\n";
    let diagnostics = check_source(source, "union.js", js_check_options());
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(ts2322.len(), 1, "got: {diagnostics:#?}");
    let expected_start = source.find("C: true").expect("test source malformed") as u32;
    assert_eq!(ts2322[0].start, expected_start);
    // Source side of the message must be the offending value's type, not
    // the enclosing object literal.
    assert!(
        ts2322[0]
            .message_text
            .starts_with("Type 'boolean' is not assignable to"),
        "unexpected message: {:?}",
        ts2322[0].message_text,
    );
}
