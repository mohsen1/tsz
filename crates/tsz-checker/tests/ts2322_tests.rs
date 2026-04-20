//! Tests for TS2322 assignability errors
//!
//! These tests verify that TS2322 "Type 'X' is not assignable to type 'Y'" errors
//! are properly emitted in various contexts.

use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::state::CheckerState;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "dom.d.ts",
        "dom.generated.d.ts",
        "dom.iterable.d.ts",
        "esnext.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                let lib_file = LibFile::from_source(file_name.to_string(), content);
                lib_files.push(Arc::new(lib_file));
                break;
            }
        }
    }

    lib_files
}

fn with_lib_contexts(source: &str, file_name: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let is_js_file = matches!(
        file_name,
        s if s.ends_with(".js")
            || s.ends_with(".jsx")
            || s.ends_with(".mjs")
            || s.ends_with(".cjs")
    );
    let lib_files = if is_js_file {
        load_lib_files_for_test()
    } else {
        Vec::new()
    };

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
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
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

/// Helper function to check if a diagnostic with a specific code was emitted
fn has_error_with_code(source: &str, code: u32) -> bool {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .any(|(d, _)| d == code)
}

/// Helper to count errors with a specific code
fn count_errors_with_code(source: &str, code: u32) -> usize {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .filter(|(d, _)| *d == code)
        .count()
}

/// Helper that returns all diagnostics for inspection
fn get_all_diagnostics(source: &str) -> Vec<(u32, String)> {
    with_lib_contexts(source, "test.ts", CheckerOptions::default())
}

fn compile_with_options(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    with_lib_contexts(source, file_name, options)
}

fn compile_with_libs_for_ts(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();

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
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
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

fn diagnostics_for_source(source: &str) -> Vec<tsz_checker::diagnostics::Diagnostic> {
    let file_name = "test.ts".to_string();
    let mut parser = ParserState::new(file_name.clone(), source.to_string());
    let root = parser.parse_source_file();
    let lib_files = load_lib_files_for_test();
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
        file_name,
        CheckerOptions::default(),
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<tsz_checker::context::LibContext> = lib_files
            .iter()
            .map(|lib| tsz_checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

// =============================================================================
// Return Statement Tests (TS2322)
// =============================================================================
include!("ts2322_tests_parts/part_00.rs");
include!("ts2322_tests_parts/part_01.rs");
include!("ts2322_tests_parts/part_02.rs");
include!("ts2322_tests_parts/part_03.rs");
include!("ts2322_tests_parts/part_04.rs");
include!("ts2322_tests_parts/part_05.rs");
include!("ts2322_tests_parts/part_06.rs");
include!("ts2322_tests_parts/part_07.rs");
include!("ts2322_tests_parts/part_08.rs");
include!("ts2322_tests_parts/part_09.rs");
include!("ts2322_tests_parts/part_10.rs");
include!("ts2322_tests_parts/part_11.rs");
