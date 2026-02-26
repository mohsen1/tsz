//! Tests for contextual typing of class expression property initializers.
//!
//! These tests verify that when a class expression is returned from a function
//! with a declared return type, static property initializers (arrow/function
//! expressions) receive contextual typing from the corresponding interface member.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: parse, bind, check with lib support and noImplicitAny enabled.
fn check_with_no_implicit_any(source: &str) -> Vec<crate::checker::diagnostics::Diagnostic> {
    let lib_files = crate::test_fixtures::load_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| tsz_binder::state::LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        no_implicit_any: true,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Class expression returned from function with declared return type should
/// contextually type static arrow property parameters.
///
/// Regression test: without per-property contextual typing in
/// `get_class_constructor_type_inner`, the arrow `(arg) => {}` would be
/// evaluated with the whole Foo interface as contextual type instead of the
/// specific `(arg: A) => void` member type, causing false TS7006.
#[test]
fn test_class_expr_static_arrow_contextual_typing() {
    let source = r#"
interface A {
    numProp: number;
}
interface Foo {
    method1(arg: A): void;
}
function getFoo(): Foo {
    return class {
        static method1 = (arg) => {
            arg.numProp = 10;
        }
    }
}
"#;
    let diagnostics = check_with_no_implicit_any(source);

    let ts7006_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 7006).collect();
    assert!(
        ts7006_errors.is_empty(),
        "Should NOT emit TS7006 for contextually typed arrow parameter 'arg', got: {ts7006_errors:?}"
    );
}

/// Same test but with function expression initializers instead of arrows.
#[test]
fn test_class_expr_static_function_expr_contextual_typing() {
    let source = r#"
interface A {
    numProp: number;
}
interface Foo {
    method1(arg: A): void;
}
function getFoo(): Foo {
    return class {
        static method1 = function(arg) {
            arg.numProp = 10;
        }
    }
}
"#;
    let diagnostics = check_with_no_implicit_any(source);

    let ts7006_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 7006).collect();
    assert!(
        ts7006_errors.is_empty(),
        "Should NOT emit TS7006 for contextually typed function expression parameter 'arg', got: {ts7006_errors:?}"
    );
}
