//! Regression tests for `TS2418` computed property value message widening.
//!
//! `tsc` displays the widened primitive type in the `TS2418` message, such as
//! `string` or `number`, instead of the literal value type like `"str"` or `1`.

use crate::test_utils::check_source_diagnostics;

fn first_2418_msg(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let diag = diags
        .iter()
        .find(|diag| diag.code == 2418)
        .unwrap_or_else(|| {
            panic!(
                "expected TS2418, got: {:?}",
                diags
                    .iter()
                    .map(|diag| (diag.code, diag.message_text.clone()))
                    .collect::<Vec<_>>()
            )
        });
    diag.message_text.clone()
}

fn assert_no_2418(source: &str) {
    let diags = check_source_diagnostics(source);
    let found: Vec<_> = diags.iter().filter(|diag| diag.code == 2418).collect();
    assert!(
        found.is_empty(),
        "unexpected TS2418: {:?}",
        found
            .iter()
            .map(|diag| &diag.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ts2418_symbol_index_string_value_widened() {
    let msg = first_2418_msg(
        r#"
interface I { [k: symbol]: number; }
const s = Symbol();
const i: I = { [s]: "str" };
"#,
    );
    assert!(
        msg.contains("'string'"),
        "expected widened 'string' in message, got: {msg}"
    );
    assert!(
        !msg.contains("\"str\""),
        "literal '\"str\"' must not appear in message, got: {msg}"
    );
}

#[test]
fn ts2418_symbol_index_number_value_widened() {
    let msg = first_2418_msg(
        r#"
interface I { [k: symbol]: string; }
const s = Symbol();
const i: I = { [s]: 42 };
"#,
    );
    assert!(
        msg.contains("'number'"),
        "expected widened 'number' in message, got: {msg}"
    );
    assert!(
        !msg.contains("'42'"),
        "literal '42' must not appear in message, got: {msg}"
    );
}

#[test]
fn ts2418_as_const_value_still_widened() {
    let msg = first_2418_msg(
        r#"
interface I { [k: symbol]: number; }
const s = Symbol();
const i: I = { [s]: "const-str" as const };
"#,
    );
    assert!(
        msg.contains("'string'"),
        "expected widened 'string' for as-const value in message, got: {msg}"
    );
}

#[test]
fn ts2418_well_known_symbol_value_widened() {
    let msg = first_2418_msg(
        r#"
interface I { [k: symbol]: number; }
const s2 = Symbol();
const i: I = { [s2]: "bad" };
"#,
    );
    assert!(
        msg.contains("'string'"),
        "expected widened 'string' for well-known-symbol value in message, got: {msg}"
    );
}

#[test]
fn ts2418_not_emitted_for_literal_string_key_matching_named_prop() {
    assert_no_2418(
        r#"
interface I { a: number; }
const i: I = { ["a"]: "wrong" };
"#,
    );
}

#[test]
fn ts2418_renamed_symbol_still_widened() {
    let msg = first_2418_msg(
        r#"
interface J { [k: symbol]: boolean; }
const mySymbol = Symbol();
const obj: J = { [mySymbol]: "nope" };
"#,
    );
    assert!(
        msg.contains("'string'"),
        "expected widened 'string' in renamed-symbol case, got: {msg}"
    );
}
