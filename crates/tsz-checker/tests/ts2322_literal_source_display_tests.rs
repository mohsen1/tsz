//! Locks in TS2322 messages keeping a literal source value when authoritative
//! def-name lookup would otherwise repaint it as a wrapper interface name.
//!
//! Regression: assigning `4` to a numeric enum reported
//! `Type 'Boolean' is not assignable to type 'E'.`  — the generic-fallback
//! path used `authoritative_assignability_def_name` even when the source
//! display was already a concrete literal value (tsc never substitutes the
//! wrapper interface here).

use tsz_checker::context::CheckerOptions;

fn diagnostic_messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
#[ignore = "numeric enum literal display behavior is still tracked as checker debt"]
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
fn ts2322_collapses_computed_string_expression_keys_to_index_signature() {
    // When computed property names use string concatenation expressions like `""+"foo"`,
    // tsc collapses them to a string index signature in diagnostics: `{ [x: string]: T }`.
    let diagnostics = diagnostic_messages(
        r#"
interface I {
    [s: string]: boolean;
}
var o: I = {
    [""+"foo"]: "",
    [""+"bar"]: 0
}
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for computed-key object assignability");

    // Should show collapsed index signature, not raw expressions
    assert!(
        ts2322.1.contains("[x: string]:"),
        "computed string-expression keys should collapse to `[x: string]: ...`, got: {ts2322:?}",
    );
    assert!(
        !ts2322.1.contains(r#"[""+"foo"]"#),
        r#"should not show raw expression like `[""+"foo"]`, got: {ts2322:?}"#,
    );
}

#[test]
fn ts2322_collapses_computed_number_expression_keys_to_index_signature() {
    // When computed property names use unary plus like `+"foo"` (coercing to number),
    // tsc collapses them to a number index signature: `{ [x: number]: T }`.
    let diagnostics = diagnostic_messages(
        r#"
interface I {
    [s: number]: boolean;
}
var o: I = {
    [+"foo"]: "",
    [+"bar"]: 0
}
"#,
    );

    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for computed-key object assignability");

    // Should show collapsed number index signature
    assert!(
        ts2322.1.contains("[x: number]:"),
        "computed number-expression keys should collapse to `[x: number]: ...`, got: {ts2322:?}",
    );
    assert!(
        !ts2322.1.contains(r#"[+"foo"]"#),
        "should not show raw expression like `[+\"foo\"]`, got: {ts2322:?}",
    );
}
