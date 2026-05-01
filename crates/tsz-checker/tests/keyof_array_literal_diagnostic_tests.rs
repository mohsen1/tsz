//! Regression tests for diagnostic display when an array literal is assigned to
//! `(keyof T)[]` for a free type parameter `T`.
//!
//! When `T` is generic with a non-trivial constraint, `keyof T` evaluates to
//! the constraint's keys union (e.g., `"a" | "b"` for `T extends { a; b }`).
//! Carrying that evaluated form into per-element TS2322 elaboration produces
//! `Type '"c"' is not assignable to type '"a" | "b"'.`, which diverges from
//! `tsc`. tsc preserves the abstract `keyof T` form and widens the literal
//! source to its primitive, emitting `Type 'string' is not assignable to type
//! 'keyof T'.`. These tests lock the abstract-form display.
//!
//! Conformance reference: `compiler/keyofIsLiteralContexualType.ts`.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

/// Per-element elaboration on `(keyof T)[]` must show the abstract `keyof T`
/// in the assignability message, not the constraint's keys union.
#[test]
fn array_literal_extra_element_shows_keyof_t_for_free_type_parameter() {
    let source = r#"
function foo<T extends { a: string; b: string }>() {
    let arr: (keyof T)[] = ["a", "b", "c"];
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diagnostics:#?}"
    );
    let (_, msg) = ts2322[0];
    assert!(
        msg.contains("'keyof T'"),
        "TS2322 target should display as 'keyof T', got: {msg}"
    );
    assert!(
        !msg.contains("'\"a\" | \"b\"'"),
        "TS2322 target must not collapse to the constraint's keys union, got: {msg}"
    );
}

/// Independence-from-name: the same fix must hold regardless of the type
/// parameter's spelling. If renaming `T` to `K` breaks the assertion, the fix
/// is hardcoded to a name and not structural.
#[test]
fn array_literal_extra_element_shows_keyof_k_for_free_type_parameter() {
    let source = r#"
function foo<K extends { a: string; b: string }>() {
    let arr: (keyof K)[] = ["a", "b", "c"];
}
"#;

    let diagnostics = compile_and_get_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322; got: {diagnostics:#?}"
    );
    let (_, msg) = ts2322[0];
    assert!(
        msg.contains("'keyof K'"),
        "TS2322 target should display as 'keyof K', got: {msg}"
    );
}
