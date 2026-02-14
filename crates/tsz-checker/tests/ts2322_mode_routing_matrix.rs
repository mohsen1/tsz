//! Focused TS2322 routing matrix tests for checker option combinations.

use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

use crate::context::CheckerOptions;
use crate::{diagnostics::diagnostic_codes, CheckerState};

fn compile_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let lib_files = Vec::<Arc<LibFile>>::new();
    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<crate::context::LibContext> = lib_files
            .iter()
            .map(|lib| crate::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn count_code(diagnostics: &[(u32, String)], code: u32) -> usize {
    diagnostics.iter().filter(|(c, _)| *c == code).count()
}

#[test]
fn test_ts2322_check_js_true_reported_without_2345_in_simple_jsdoc_mismatch() {
    let source = r#"
        /** @type {number} */
        const n = "bad";
    "#;

    let diags = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322 = count_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    let ts2345 = count_code(&diags, 2345);

    assert_eq!(ts2322, 1);
    assert_eq!(ts2345, 0);
}

#[test]
fn test_ts2322_check_js_false_suppresses_jsdoc_mismatch() {
    let source = r#"
        /** @type {number} */
        const n = "bad";
    "#;

    let diags = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(count_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE), 0);
    assert_eq!(count_code(&diags, 2345), 0);
}

#[test]
fn test_target_sensitive_check_js_strictness_stability() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const n = null;
    "#;

    let strict = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let loose = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            ..CheckerOptions::default()
        },
    );

    assert!(count_code(&strict, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE) >= 1);
    assert!(count_code(&strict, 2345) == 0);
    assert!(count_code(&loose, 2345) == 0);
}
