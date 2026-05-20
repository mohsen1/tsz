//! Locks in TS2322 messages keeping a literal source value when authoritative
//! def-name lookup would otherwise repaint it as a wrapper interface name.
//!
//! Regression: assigning `4` to a numeric enum reported
//! `Type 'Boolean' is not assignable to type 'E'.`  — the generic-fallback
//! path used `authoritative_assignability_def_name` even when the source
//! display was already a concrete literal value (tsc never substitutes the
//! wrapper interface here).

fn diagnostic_messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

#[test]
fn ts2322_numeric_literal_to_enum_keeps_literal_source_display() {
    let src = r#"
enum E { A, B, C }
declare let e: E;
e = 4;
"#;
    let diagnostics = diagnostic_messages(src);
    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for `e = 4`");
    assert!(
        ts2322.1.contains("Type '4'"),
        "TS2322 should display source as literal `4`, got: {ts2322:?}"
    );
    assert!(
        !ts2322.1.contains("'Boolean'"),
        "TS2322 must not repaint a numeric literal as `Boolean`, got: {ts2322:?}"
    );
}

#[test]
fn ts2322_string_literal_to_string_literal_keeps_literal_source_display() {
    let src = r#"
declare let s: "foo";
s = "bar";
"#;
    let diagnostics = diagnostic_messages(src);
    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for `s = \"bar\"`");
    assert!(
        ts2322.1.contains("Type '\"bar\"'"),
        "TS2322 should display source as quoted literal, got: {ts2322:?}"
    );
}

#[test]
fn ts2322_boolean_literal_to_enum_keeps_literal_source_display() {
    let src = r#"
enum E { A, B, C }
declare let e: E;
e = true as any as E | boolean;
e = (false as any) as E | boolean;
"#;
    // Sanity: assigning bool union should not be flagged as TS2322; the
    // important assertion is that no diagnostic mislabels `true`/`false`.
    let diagnostics = diagnostic_messages(src);
    for (code, msg) in &diagnostics {
        if *code == 2322 {
            assert!(
                !msg.contains("Type 'Boolean'") || msg.contains("Type 'Boolean' "),
                "TS2322 wrapper-interface confusion regressed: {msg}"
            );
        }
    }
}

#[test]
fn ts2322_function_type_parameter_object_display_has_single_trailing_semicolon() {
    let diagnostics = diagnostic_messages(
        r#"
class Base { foo!: string; }
class Derived extends Base { bar!: string; }

declare var a8: (x: (arg: Base) => Derived, y: (arg2: Base) => Derived) => (r: Base) => Derived;
declare var b8: <T extends Base, U extends Derived>(x: (arg: T) => U, y: (arg2: { foo: number; }) => U) => (r: T) => U;
a8 = b8;
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for incompatible callback parameter object type");

    assert!(
        ts2322.1.contains("{ foo: number; }"),
        "TS2322 should preserve a single object-type semicolon, got: {ts2322:?}",
    );
    assert!(
        !ts2322.1.contains("number;;"),
        "TS2322 must not duplicate object-type semicolons, got: {ts2322:?}",
    );
}

#[test]
fn ts2322_preserves_computed_unique_symbol_object_key_display() {
    let diagnostics = diagnostic_messages(
        r#"
const sym = Symbol();

function gg2(x: { [key: symbol]: string }, y: { [sym]: number }) {
    x = y;
}
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for computed unique-symbol object assignability");

    assert!(
        ts2322.1.contains("Type '{ [sym]: number; }'"),
        "computed-key TS2322 should show source as `{{ [sym]: number; }}`, got: {ts2322:?}",
    );
    assert!(
        !ts2322.1.contains("Type '{ __unique_"),
        "TS2322 should not hide computed keys behind synthetic `__unique_` names, got: {ts2322:?}",
    );
}

#[test]
fn ts2322_normalizes_contextual_computed_string_keys_to_index_signature_display() {
    let diagnostics = diagnostic_messages(
        r#"
interface I {
    [s: string]: boolean;
    [s: number]: boolean;
}

var o: I = {
    [""+"foo"]: "",
    [""+"bar"]: 0,
    [""+"baz"]: ""
};
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for contextual computed string keys");

    assert!(
        ts2322
            .1
            .contains("Type '{ [x: string]: string | number; }'"),
        "contextual computed string keys should render as a string index signature, got: {ts2322:?}",
    );
    assert!(
        !ts2322.1.contains("[\"\"+\"foo\"]"),
        "TS2322 should not expose expanded computed string key text, got: {ts2322:?}",
    );
}

#[test]
fn ts2322_normalizes_contextual_computed_number_keys_to_index_signature_display() {
    let diagnostics = diagnostic_messages(
        r#"
interface I {
    [s: number]: boolean;
}

var o: I = {
    [+"foo"]: 0,
    [+"bar"]: ""
};
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for contextual computed number keys");

    assert!(
        ts2322
            .1
            .contains("Type '{ [x: number]: string | number; }'"),
        "contextual computed number keys should render as a number index signature, got: {ts2322:?}",
    );
    assert!(
        !ts2322.1.contains("[+\"foo\"]"),
        "TS2322 should not expose expanded computed number key text, got: {ts2322:?}",
    );
}
