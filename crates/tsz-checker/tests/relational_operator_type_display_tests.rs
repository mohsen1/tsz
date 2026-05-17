//! Structural rule: for any relational-operator mismatch (TS2365), the displayed
//! type name comes from `format_type_for_operator_display(operand_type)`, not
//! from raw AST annotation text.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_messages_with_code};

const TS2365: u32 = 2365;

fn relational_messages(source: &str) -> Vec<String> {
    let diags = check_source_diagnostics(source);
    diagnostic_messages_with_code(&diags, TS2365)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn assert_types_in_message(msgs: &[String], left: &str, right: &str) {
    assert!(
        msgs.iter().any(|m| m.contains(left) && m.contains(right)),
        "expected message with {left} and {right}, got: {msgs:?}"
    );
}

#[test]
fn relational_number_less_than_boolean_shows_concrete_types() {
    let msgs = relational_messages("var a: number; var b: boolean; var r = a < b;");
    assert_types_in_message(&msgs, "'number'", "'boolean'");
}

#[test]
fn relational_boolean_less_than_number_shows_concrete_types() {
    let msgs = relational_messages("var a: number; var b: boolean; var r = b < a;");
    assert_types_in_message(&msgs, "'boolean'", "'number'");
}

#[test]
fn relational_string_less_than_number_shows_concrete_types() {
    let msgs = relational_messages("var a: number; var str: string; var r = str < a;");
    assert_types_in_message(&msgs, "'string'", "'number'");
}

#[test]
fn all_relational_operators_emit_ts2365_for_number_boolean() {
    let source = r#"
var a: number;
var b: boolean;
var r1 = a < b;
var r2 = a <= b;
var r3 = a >= b;
var r4 = a > b;
"#;
    let msgs = relational_messages(source);
    assert_eq!(
        msgs.len(),
        4,
        "expected exactly 4 TS2365 errors for four relational ops, got: {msgs:?}"
    );
    for msg in &msgs {
        assert!(
            msg.contains("'number'") && msg.contains("'boolean'"),
            "expected 'number' and 'boolean' in every message, got: {msg}"
        );
    }
}

/// Type parameter names (not annotation text) must appear in the message.
/// Uses non-standard names `Tp`/`Uq` to rule out any hardcoded `T` check.
#[test]
fn relational_type_param_named_tp_shows_type_param_name() {
    let source = r#"
function fn1<Tp, Uq>(a: Tp, b: Uq): boolean {
    return a < b;
}
"#;
    let msgs = relational_messages(source);
    assert!(
        !msgs.is_empty(),
        "expected at least one TS2365 error for Tp < Uq"
    );
    assert_types_in_message(&msgs, "'Tp'", "'Uq'");
}

#[test]
fn relational_type_param_named_k_v_shows_type_param_names() {
    let source = r#"
function fn2<K, V>(k: K, v: V): boolean {
    return k < v;
}
"#;
    let msgs = relational_messages(source);
    assert!(!msgs.is_empty(), "expected at least one TS2365 for K < V");
    assert_types_in_message(&msgs, "'K'", "'V'");
}

/// Short type alias (≤3 chars) as annotation: formatter resolves the alias to
/// its expanded type. Regression guard: the old code sliced raw AST annotation
/// bytes — after the parser node-span fix that would silently return the alias
/// name instead of the resolved type; the formatter must always drive display.
#[test]
fn relational_short_alias_as_param_annotation_shows_formatted_types() {
    let source = r#"
type Tp = boolean;
function fn3(a: Tp, b: number): boolean {
    return a < b;
}
"#;
    let msgs = relational_messages(source);
    assert!(
        !msgs.is_empty(),
        "expected at least one TS2365 for Tp < number"
    );
    assert_types_in_message(&msgs, "'boolean'", "'number'");
}

#[test]
fn relational_short_alias_named_fn_shows_formatted_types() {
    let source = r#"
type Fn = boolean;
function fn4(x: Fn, y: number): boolean {
    return x < y;
}
"#;
    let msgs = relational_messages(source);
    assert!(
        !msgs.is_empty(),
        "expected at least one TS2365 for Fn < number"
    );
    assert_types_in_message(&msgs, "'boolean'", "'number'");
}
