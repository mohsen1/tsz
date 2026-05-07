//! Regression coverage for string-literal union display order in
//! assignability diagnostics.
//!
//! Reproduces the `typeArgumentsWithStringLiteralTypes01` conformance failure
//! where `takeReturnHelloWorld(str: "Hello" | "World"): "Hello" | "World"`
//! produced a diagnostic message containing `'"World" | "Hello"'`. tsc
//! preserves source declaration order in the message: `'"Hello" | "World"'`.

use crate::test_utils::check_source_diagnostics;

#[test]
fn string_literal_union_display_preserves_source_order_in_ts2322() {
    let diags = check_source_diagnostics(
        r#"
declare function takeReturnHelloWorld(str: "Hello" | "World"): "Hello" | "World";
let h: "Hello" = takeReturnHelloWorld("Hello");
"#,
    );

    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let msg = &ts2322[0].message_text;
    assert!(
        msg.contains("\"Hello\" | \"World\""),
        "expected source-order union '\"Hello\" | \"World\"' in TS2322 message, got: {msg}"
    );
    assert!(
        !msg.contains("\"World\" | \"Hello\""),
        "did not expect reversed-order '\"World\" | \"Hello\"' in TS2322 message, got: {msg}"
    );
}

/// Reduced from `typeArgumentsWithStringLiteralTypes01.ts`: when a
/// function is instantiated with type arguments in REVERSED order
/// (`fun2<"World", "Hello">`), the inferred return type's literal-union
/// must still display in source-declaration order (`"Hello" | "World"`).
#[test]
fn string_literal_union_display_reversed_type_args() {
    let diags = check_source_diagnostics(
        r#"
declare function takeReturnHello(str: "Hello"): "Hello";

function fun2<T, U>(x: T, y: U): T | U {
    return x;
}

let c = fun2<"World", "Hello">("Hello", "Hello");
c = takeReturnHello(c);
"#,
    );

    let bad: Vec<_> = diags
        .iter()
        .filter(|d| d.message_text.contains("\"World\" | \"Hello\""))
        .collect();
    assert!(
        bad.is_empty(),
        "no diagnostic should flip union order to '\"World\" | \"Hello\"'; got: {:?}",
        bad.iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
