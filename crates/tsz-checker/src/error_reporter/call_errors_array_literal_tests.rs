use crate::test_utils::check_source_diagnostics;

#[test]
fn ts2345_array_literal_call_argument_display_widens_boolean_literal_element() {
    let source = r#"
declare const test1:
  | ((...args: [a: string | number, b: number | boolean]) => void)
  | ((...args: [c: number | boolean, d: string | boolean]) => void);

test1(42, [true]);
"#;

    let diagnostics = check_source_diagnostics(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345");

    assert!(
        diag.message_text.contains(
            "Argument of type 'boolean[]' is not assignable to parameter of type 'boolean'."
        ),
        "TS2345 should widen boolean literal array elements for non-literal targets, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("Argument of type 'true[]'"),
        "TS2345 should not preserve boolean literal array elements here, got: {diag:?}"
    );
}
