//! Tests for TS2322 diagnostic messages showing the declared annotation text
//! for class property declarations, even when the annotation references
//! unresolved type names.
//!
//! Regression: for class properties like `public f: () => SymbolScope = null`,
//! when `SymbolScope` is undefined (TS2304), the TS2322 message was incorrectly
//! showing `() => error` (the evaluated type with our internal error sentinel)
//! instead of `() => SymbolScope` (the source annotation text).
//!
//! Root cause: `declared_type_annotation_text_for_expression_with_options` used
//! scope-chain resolution (`resolve_identifier_symbol`) which correctly filters
//! out class member symbols to avoid false positive identifier resolution in
//! expression contexts. But for declaration-site lookup, we need the direct
//! `node_symbols` mapping, which always maps declaration-site identifiers to
//! their declared symbols.

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        tsz_checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

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
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();

    assert!(!ts2322.is_empty(), "expected at least one TS2322");
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
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();

    assert_eq!(
        ts2322.len(),
        2,
        "expected two TS2322 errors, got: {ts2322:?}"
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
