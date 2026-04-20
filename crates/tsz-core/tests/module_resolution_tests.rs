//! Cross-file module resolution tests.
//!
//! Tests for all forms of cross-file module references:
//! - ES imports: `import { x } from "./module"`
//! - Default imports: `import x from "./module"`
//! - Namespace imports: `import * as ns from "./module"`
//! - Re-exports: `export { x } from "./module"`
//! - Require: `const x = require("./module")`
//! - Import equals: `import x = require("./module")`
//! - Dynamic import: `const x = await import("./module")`
//! - Triple-slash reference directives: `/// <reference path="./module.ts" />`
//! - AMD define: `define(["./module"], function(m) { ... })`

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::TypeInterner;

// =============================================================================
// Test Helpers
// =============================================================================

type ModuleExportsFixture<'a> = Vec<(&'a str, Vec<(&'a str, u32)>)>;

/// Helper to parse, bind, and check a single file with `module_exports` pre-populated.
/// Returns the checker diagnostics as (code, message) pairs.
///
/// `module_exports` maps module specifier -> list of export names.
/// Each module's exports are created by parsing and binding a synthetic exporter file.
fn check_with_module_exports(
    source: &str,
    file_name: &str,
    module_exports: ModuleExportsFixture<'_>,
    report_unresolved_imports: bool,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors in {}: {:?}",
        file_name,
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);

    // Pre-populate module_exports by parsing synthetic exporter files
    for (module_name, exports) in &module_exports {
        // Generate a source with all the exports
        let export_source: String = exports
            .iter()
            .map(|(name, _)| format!("export const {name} = 0;"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut export_parser = ParserState::new(format!("{module_name}.ts"), export_source);
        let export_root = export_parser.parse_source_file();
        let mut export_binder = BinderState::new();
        export_binder.bind_source_file(export_parser.get_arena(), export_root);

        let mut table = crate::binder::SymbolTable::new();
        for (name, _) in exports {
            if let Some(sym_id) = export_binder.file_locals.get(name) {
                table.set(name.to_string(), sym_id);
            }
        }
        binder.module_exports.insert(module_name.to_string(), table);
    }

    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: crate::common::ModuleKind::CommonJS,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);

    // Enable unresolved import reporting if requested
    checker.ctx.report_unresolved_imports = report_unresolved_imports;

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Helper similar to `check_with_module_exports` but allows specifying custom source
/// for each module. This is useful for testing with class exports, namespace patterns, etc.
pub fn check_with_module_sources(
    source: &str,
    file_name: &str,
    module_sources: Vec<(&str, &str)>,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors in {}: {:?}",
        file_name,
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);

    // Pre-populate module_exports by parsing the provided module sources
    for (module_name, module_source) in &module_sources {
        let mut export_parser =
            ParserState::new(format!("{module_name}.ts"), module_source.to_string());
        let export_root = export_parser.parse_source_file();
        assert!(
            export_parser.get_diagnostics().is_empty(),
            "Parse errors in {}.ts: {:?}",
            module_name,
            export_parser.get_diagnostics()
        );
        let mut export_binder = BinderState::new();
        merge_shared_lib_symbols(&mut export_binder);
        export_binder.bind_source_file(export_parser.get_arena(), export_root);

        let mut table = crate::binder::SymbolTable::new();
        for (name, &sym_id) in export_binder.file_locals.iter() {
            table.set(name.clone(), sym_id);
        }
        binder.module_exports.insert(module_name.to_string(), table);
    }

    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: crate::common::ModuleKind::CommonJS,
        ..Default::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Helper to parse, bind, and check a single file with `resolved_modules` set.
/// This simulates the CLI driver having resolved certain module specifiers.
fn check_with_resolved_modules(
    source: &str,
    file_name: &str,
    resolved_modules: Vec<&str>,
    unresolved_modules: Vec<&str>,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors in {}: {:?}",
        file_name,
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    checker.ctx.report_unresolved_imports = true;
    let modules: rustc_hash::FxHashSet<String> = resolved_modules
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    checker.ctx.set_resolved_modules(modules);

    // Simulate unresolved modules by setting resolved_module_errors
    let mut errors: rustc_hash::FxHashMap<
        (usize, String),
        crate::checker::context::ResolutionError,
    > = rustc_hash::FxHashMap::default();
    for module_name in unresolved_modules {
        errors.insert(
            (0, module_name.to_string()),
            crate::checker::context::ResolutionError {
                code: TS2882,
                message: format!(
                    "Cannot find module or type declarations for side-effect import of '{module_name}'."
                ),
            },
        );
    }
    checker
        .ctx
        .set_resolved_module_errors(std::sync::Arc::new(errors));

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Like `check_with_resolved_modules` but accepts custom `CheckerOptions`.
fn check_with_resolved_modules_opts(
    source: &str,
    file_name: &str,
    resolved_modules: Vec<&str>,
    unresolved_modules: Vec<&str>,
    opts: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors in {}: {:?}",
        file_name,
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);

    checker.ctx.report_unresolved_imports = true;
    let modules: rustc_hash::FxHashSet<String> = resolved_modules
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    checker.ctx.set_resolved_modules(modules);

    let mut errors: rustc_hash::FxHashMap<
        (usize, String),
        crate::checker::context::ResolutionError,
    > = rustc_hash::FxHashMap::default();
    for module_name in unresolved_modules {
        errors.insert(
            (0, module_name.to_string()),
            crate::checker::context::ResolutionError {
                code: TS2882,
                message: format!(
                    "Cannot find module or type declarations for side-effect import of '{module_name}'."
                ),
            },
        );
    }
    checker
        .ctx
        .set_resolved_module_errors(std::sync::Arc::new(errors));

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Like `check_with_resolved_modules_opts`, but unresolved modules surface as TS2307.
///
/// This exercises the checker-side fallback that rewrites unresolved Node built-ins
/// into the TS2580/TS2591 family.
fn check_with_module_not_found_errors(
    source: &str,
    file_name: &str,
    resolved_modules: Vec<&str>,
    unresolved_modules: Vec<&str>,
    opts: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors in {}: {:?}",
        file_name,
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);

    checker.ctx.report_unresolved_imports = true;
    let modules: rustc_hash::FxHashSet<String> = resolved_modules
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    checker.ctx.set_resolved_modules(modules);

    let mut errors: rustc_hash::FxHashMap<
        (usize, String),
        crate::checker::context::ResolutionError,
    > = rustc_hash::FxHashMap::default();
    for module_name in unresolved_modules {
        errors.insert(
            (0, module_name.to_string()),
            crate::checker::context::ResolutionError {
                code: TS2307,
                message: format!(
                    "Cannot find module '{module_name}' or its corresponding type declarations."
                ),
            },
        );
    }
    checker
        .ctx
        .set_resolved_module_errors(std::sync::Arc::new(errors));

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}
include!("module_resolution_tests_parts/part_00.rs");
include!("module_resolution_tests_parts/part_01.rs");
include!("module_resolution_tests_parts/part_02.rs");
include!("module_resolution_tests_parts/part_03.rs");
include!("module_resolution_tests_parts/part_04.rs");
include!("module_resolution_tests_parts/part_05.rs");
include!("module_resolution_tests_parts/part_06.rs");
include!("module_resolution_tests_parts/part_07.rs");
include!("module_resolution_tests_parts/part_08.rs");
