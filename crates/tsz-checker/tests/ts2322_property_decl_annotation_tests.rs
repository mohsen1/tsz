//! Tests for diagnostic display when a class property annotation references an
//! unresolved type name.
//!
//! Regression: for class properties like `public f: () => SymbolScope = null`,
//! when `SymbolScope` is undefined (TS2304), the TS2322 message was incorrectly
//! showing `() => error` (the evaluated type with our internal error sentinel)
//! instead of `() => SymbolScope` (the source annotation text).
//!
//! After the lowering fix in PR #1464, the unresolved type-position identifier
//! lowers to `UnresolvedTypeName` instead of `TypeId::ERROR`, so the `error`
//! sentinel never leaks into a cascading message. These tests pin the cascade
//! invariant too: tsc still reports TS2322 for `null` assigned to a function or
//! class type whose nested members mention an unresolved type.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

/// Class property with a function type annotation that references an unresolved
/// type should show the annotation text in TS2322, not the internal `error` sentinel.
#[test]
fn ts2322_class_prop_func_annotation_with_unresolved_return_type() {
    // `SymbolScope` is not defined → TS2304. The TS2322 for `null` assignment
    // must display `() => SymbolScope`, not `() => error`.
    let source = r#"
class EnclosingScopeContext {
    public scopeGetter: () => SymbolScope = null;
}
"#;
    let diags = get_diagnostics(source);

    // TS2304 must fire for the unresolved name, and TS2322 must still fire for
    // the nullish assignment because the target's top-level shape is callable.
    let ts2304: Vec<_> = diags.iter().filter(|(code, _)| *code == 2304).collect();
    assert!(
        !ts2304.is_empty(),
        "expected TS2304 'Cannot find name SymbolScope', got: {diags:?}"
    );

    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected one cascading TS2322 for null assigned to () => SymbolScope, got: {diags:?}"
    );
    for (_, msg) in &ts2322 {
        assert!(
            !msg.contains("error"),
            "TS2322 message should not contain internal 'error' sentinel: {msg}"
        );
        assert!(
            msg.contains("SymbolScope"),
            "TS2322 message should show annotation text '() => SymbolScope': {msg}"
        );
    }
}

/// Two class properties with unresolved types in their function annotations.
#[test]
fn ts2322_class_prop_multiple_func_annotations_with_unresolved_types() {
    let source = r#"
class Context {
    public scopeGetter: () => SymbolScope = null;
    public objectLiteralScopeGetter: () => SymbolScope = null;
}
"#;
    let diags = get_diagnostics(source);

    // One TS2304 and one cascading TS2322 per declaration must fire.
    let ts2304: Vec<_> = diags.iter().filter(|(code, _)| *code == 2304).collect();
    assert!(
        ts2304.len() >= 2,
        "expected at least two TS2304 errors (one per declaration), got: {diags:?}"
    );

    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        2,
        "expected one TS2322 per null-initialized function property, got: {diags:?}"
    );
    for (_, msg) in &ts2322 {
        assert!(
            !msg.contains("error"),
            "TS2322 message must not contain 'error' sentinel: {msg}"
        );
        assert!(
            msg.contains("SymbolScope"),
            "TS2322 message must show annotation '() => SymbolScope': {msg}"
        );
    }
}

/// Returning `null` from a function whose declared class return type contains
/// nested unresolved members should still emit TS2322 at the return statement.
#[test]
fn ts2322_return_null_to_class_type_with_unresolved_member_type() {
    let source = r#"
class EnclosingScopeContext {
    public scopeGetter: () => SymbolScope = null;
}
function findEnclosingScopeAt(): EnclosingScopeContext {
    return null;
}
"#;
    let diags = get_diagnostics(source);

    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        ts2322.iter().any(|(_, msg)| {
            msg.contains("Type 'null' is not assignable to type 'EnclosingScopeContext'")
        }),
        "expected TS2322 for return null against class type with nested unresolved member, got: {diags:?}"
    );
    for (_, msg) in &ts2322 {
        assert!(
            !msg.contains("error"),
            "TS2322 message must not contain internal 'error' sentinel: {msg}"
        );
    }
}

/// When the annotation type IS resolved, no regression: should still work normally.
#[test]
fn ts2322_class_prop_func_annotation_resolved_type() {
    let source = r#"
class Context {
    public getter: () => string = null;
}
"#;
    let diags = get_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();

    assert!(!ts2322.is_empty(), "expected at least one TS2322");
    for (_, msg) in &ts2322 {
        assert!(
            msg.contains("string"),
            "TS2322 should show the resolved type '() => string': {msg}"
        );
    }
}
