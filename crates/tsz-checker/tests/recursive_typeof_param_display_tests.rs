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
