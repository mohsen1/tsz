//! Tests for generic call-argument diagnostics whose parameter is constrained
//! by `keyof` another argument.
//!
//! Structural rule: when a generic call parameter instantiates to the same key
//! space as `keyof` a named argument type, TS2345 should render the parameter
//! as `keyof <named type>` instead of the expanded finite literal-key union.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

fn ts2345_messages(source: &str) -> Vec<String> {
    check_strict(source)
        .into_iter()
        .filter(|diag| diag.code == 2345)
        .map(|diag| diag.message_text)
        .collect()
}

#[test]
fn generic_keyof_parameter_displays_keyof_named_argument_type() {
    let messages = ts2345_messages(
        r#"
class Shape {
    name: string;
    width: number;
    height: number;
    visible: boolean;
}

function getProperty<T, K extends keyof T>(obj: T, key: K) {
    return obj[key];
}

declare const shape: Shape;
getProperty(shape, "size");
"#,
    );

    assert_eq!(messages.len(), 1, "expected one TS2345, got: {messages:?}");
    assert!(
        messages[0].contains("parameter of type 'keyof Shape'"),
        "expected `keyof Shape` parameter display, got: {:?}",
        messages[0]
    );
    assert!(
        !messages[0].contains("\"name\" | \"width\" | \"height\" | \"visible\""),
        "parameter display must not expand to the literal key union, got: {:?}",
        messages[0]
    );
}

#[test]
fn generic_keyof_parameter_displays_keyof_named_argument_type_renamed() {
    let messages = ts2345_messages(
        r#"
interface RecordShape {
    alpha: string;
    beta: number;
}

function readField<Obj, Field extends keyof Obj>(value: Obj, field: Field) {
    return value[field];
}

declare const value: RecordShape;
readField(value, "gamma");
"#,
    );

    assert_eq!(messages.len(), 1, "expected one TS2345, got: {messages:?}");
    assert!(
        messages[0].contains("parameter of type 'keyof RecordShape'"),
        "expected renamed `keyof RecordShape` display, got: {:?}",
        messages[0]
    );
    assert!(
        !messages[0].contains("\"alpha\" | \"beta\""),
        "parameter display must not depend on expanded key spelling, got: {:?}",
        messages[0]
    );
}
