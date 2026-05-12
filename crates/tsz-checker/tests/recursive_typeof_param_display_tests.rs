//! Diagnostic display for self-referential `typeof` in function parameter
//! positions.
//!
//! When a function parameter's declared type is `typeof X` and `X`'s value
//! type IS the enclosing function (e.g. `static g(t: typeof C.g)`), the
//! checker's assignability-display normalization used to evaluate the
//! `TypeQuery` — substituting it with the resolved function shape — and then
//! re-traverse that shape, producing an extra outer wrapper:
//!
//!   `(t: (t: typeof g) => void) => void`
//!
//! tsc preserves the typeof reference instead:
//!
//!   `(t: typeof g) => void`
//!
//! This regression test pins the parity.

use tsz_checker::context::CheckerOptions;

fn check_strict(source: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..Default::default()
    };
    tsz_checker::test_utils::check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn recursive_typeof_param_preserved_in_ts2345_message() {
    let source = r#"
class C {
    static g(t: typeof C.g) { }
}
C.g(3);
"#;
    let diags = check_strict(source);
    let ts2345: Vec<_> = diags.iter().filter(|(c, _)| *c == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Expected one TS2345 for `C.g(3)`, got: {diags:?}"
    );
    let msg = &ts2345[0].1;
    assert!(
        msg.contains("(t: typeof g) => void"),
        "Expected the parameter type to display as `(t: typeof g) => void`, got: {msg}"
    );
    assert!(
        !msg.contains("(t: (t: typeof g) => void)"),
        "Expected NO doubly-wrapped parameter (typeof should not be expanded inside its own value type), got: {msg}"
    );
}

fn diagnostics_for(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

#[test]
fn recursive_typeof_function_target_display_elides_nested_return_cycle() {
    let diagnostics = diagnostics_for(
        r#"
var f4: () => typeof f4;
f4 = 3;
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for assigning number to recursive function type");
    assert!(
        diag.message_text
            .contains("Type 'number' is not assignable to type '() => ...'."),
        "recursive typeof function target should elide the nested return cycle, got: {diag:?}"
    );
}

#[test]
fn recursive_overloaded_typeof_parameter_display_uses_callable_surface() {
    let diagnostics = diagnostics_for(
        r#"
function f6(): typeof f6;
function f6(a: typeof f6): () => number;
function f6(a?: any) { return f6; }

f6("");
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2345)
        .expect("expected TS2345 for string argument");
    assert!(
        diag.message_text.contains(
            "Argument of type 'string' is not assignable to parameter of type '{ (): typeof f6; (a: typeof f6): () => number; }'."
        ),
        "recursive overloaded typeof parameter should use callable object surface, got: {diag:?}"
    );
}

#[test]
fn non_recursive_nested_typeof_function_return_does_not_elide() {
    let diagnostics = diagnostics_for(
        r#"
declare function refFn(v: number): number;
declare let expected: () => (x: typeof refFn) => number;
expected = 3;
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for assigning number to nested function-return type");
    assert!(
        diag.message_text
            .contains("() => (x: typeof refFn) => number"),
        "non-recursive nested typeof in function return should keep full surface, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("() => ..."),
        "non-recursive nested typeof in function return must not be over-elided, got: {diag:?}"
    );
}
