//! Tests for TS2774 ("This condition will always return true since this function
//! is always defined. Did you mean to call it instead?")
//!
//! TS2774 fires when a callable value that cannot be nullish is used in a
//! truthiness position (if-condition, ternary, &&) without being invoked.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.es2015.d.ts"),
        manifest_dir.join("../../TypeScript/lib/lib.dom.d.ts"),
    ];

    let mut lib_files = Vec::new();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            lib_files.push(Arc::new(LibFile::from_source(file_name, content)));
        }
    }
    lib_files
}

/// Check source with strictNullChecks enabled and return diagnostics.
fn check_strict(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_lib_files();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| tsz_binder::state::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default(); // strict_null_checks = true by default

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
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Check source WITHOUT strictNullChecks and return diagnostics.
fn check_non_strict(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_lib_files();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| tsz_binder::state::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        strict_null_checks: false,
        ..CheckerOptions::default()
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
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn has_ts2774(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.code == 2774)
}

#[test]
fn ts2774_required_function_param_in_if() {
    let diags = check_strict("function test(fn: () => boolean) {\n    if (fn) {}\n}\n");
    assert!(
        has_ts2774(&diags),
        "TS2774 should fire for non-optional callable in if-condition"
    );
}

#[test]
fn ts2774_no_error_for_optional_function_param() {
    let diags = check_strict("function test(fn?: () => boolean) {\n    if (fn) {}\n}\n");
    assert!(
        !has_ts2774(&diags),
        "TS2774 should NOT fire for optional callable (type includes undefined)"
    );
}

#[test]
fn ts2774_no_error_when_called_in_body() {
    let diags = check_strict(
        "function test(fn: () => boolean) {\n    if (fn) {\n        fn();\n    }\n}\n",
    );
    assert!(
        !has_ts2774(&diags),
        "TS2774 should NOT fire when the function is called in the body"
    );
}

#[test]
fn ts2774_nested_function_declaration() {
    let diags = check_strict(
        "function test() {\n    function inner() { return true; }\n    if (inner) {}\n}\n",
    );
    assert!(
        has_ts2774(&diags),
        "TS2774 should fire for nested function declarations used in if-condition"
    );
}

#[test]
fn ts2774_no_error_without_strict_null_checks() {
    let diags = check_non_strict("function test(fn: () => boolean) {\n    if (fn) {}\n}\n");
    assert!(
        !has_ts2774(&diags),
        "TS2774 should NOT fire without strictNullChecks"
    );
}

#[test]
fn ts2774_declare_function() {
    let diags = check_strict("declare function test(): boolean;\nif (test) {}\n");
    assert!(
        has_ts2774(&diags),
        "TS2774 should fire for declared functions"
    );
}
