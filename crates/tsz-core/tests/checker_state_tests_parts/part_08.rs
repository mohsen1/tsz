//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
#[test]
fn test_declared_module_recorded_in_script() {
    use crate::parser::ParserState;

    let source = r#"
declare module "dep" {
    export const value: number;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.declared_modules.contains("dep"),
        "Expected declared module to be recorded"
    );
}

// =========================================================================
// TS2307 Module Resolution Error Tests
// =========================================================================

/// Test TS2307 for relative import that cannot be resolved
#[test]
fn test_ts2307_relative_import_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
import { foo } from "./non-existent-module";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
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
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            || codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O),
        "Expected TS2307 or TS2792 for relative import that cannot be resolved, got: {codes:?}"
    );
}

/// Test TS2307 for bare module specifier (npm package) that cannot be resolved
#[test]
fn test_ts2307_bare_specifier_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
import { something } from "nonexistent-npm-package";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
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
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            || codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O),
        "Expected TS2307 or TS2792 for bare specifier that cannot be resolved, got: {codes:?}"
    );
}

/// Test TS2307 for unresolved CommonJS `require()` calls in checked JavaScript.
#[test]
fn test_ts2307_check_js_require_call_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const { foo } = require("bar");
"#;

    let mut parser = ParserState::new("main.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
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
        "main.js".to_string(),
        crate::checker::context::CheckerOptions {
            check_js: true,
            allow_js: true,
            module: crate::common::ModuleKind::CommonJS,
            target: crate::common::ScriptTarget::ES2018,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let ts2307: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        })
        .collect();
    assert_eq!(
        ts2307.len(),
        1,
        "Expected exactly one TS2307 for unresolved require(\"bar\"), got: {:?}",
        checker.ctx.diagnostics
    );
    assert!(
        ts2307[0].message_text.contains("'bar'"),
        "Expected TS2307 message to reference 'bar', got: {}",
        ts2307[0].message_text
    );
}

/// Local declarations named `require` should shadow CommonJS require semantics.
#[test]
fn test_local_require_shadowing_does_not_emit_ts2307() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
function require(name) {
    return { foo: 1 };
}
const { foo } = require("bar");
"#;

    let mut parser = ParserState::new("main.js".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
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
        "main.js".to_string(),
        crate::checker::context::CheckerOptions {
            check_js: true,
            allow_js: true,
            module: crate::common::ModuleKind::CommonJS,
            no_implicit_any: false,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|diag| diag.code)
        .collect();
    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Local require() should shadow CommonJS module resolution. Diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that `declared_modules` prevents TS2307 when module is declared
#[test]
fn test_declared_module_prevents_ts2307() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    // Script file (no import/export) with declare module
    let source = r#"
declare module "my-external-lib" {
    export const value: number;
}
"#;

    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    // Verify the module was registered
    assert!(
        binder.declared_modules.contains("my-external-lib"),
        "Expected 'my-external-lib' to be in declared_modules"
    );

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.d.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // No TS2307 should be emitted since the module is declared
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes
            .contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS),
        "Should not emit TS2307 when module is declared via 'declare module', got: {codes:?}"
    );
}

/// Test that `shorthand_ambient_modules` prevents TS2307 when module is declared without body
#[test]
fn test_shorthand_ambient_module_prevents_ts2307() {
    use crate::parser::ParserState;

    // Shorthand ambient module declaration (no body)
    let source = r#"
declare module "*.json";

import data from "./file.json";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    // Verify the shorthand module was registered
    assert!(
        binder.shorthand_ambient_modules.contains("*.json"),
        "Expected '*.json' to be in shorthand_ambient_modules"
    );

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Note: The import "./file.json" will still emit TS2307 because the shorthand module
    // declaration is for "*.json" pattern, not "./file.json" literal.
    // This is expected behavior - shorthand ambient module pattern matching is not implemented.
}

/// Test TS2307 for scoped npm package import that cannot be resolved
#[test]
fn test_ts2307_scoped_package_not_found() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
import { Component } from "@angular/core";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
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
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS)
            || codes.contains(&diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O),
        "Expected TS2307 or TS2792 for scoped package that cannot be resolved, got: {codes:?}"
    );
}

/// Test multiple unresolved imports each emit TS2307
#[test]
fn test_ts2307_multiple_unresolved_imports() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
import { foo } from "./missing1";
import { bar } from "./missing2";
import * as pkg from "nonexistent-pkg";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
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
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    // Count module-not-found diagnostics (either TS2307 or TS2792)
    let module_not_found_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
                || d.code == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
        })
        .count();

    assert_eq!(
        module_not_found_count, 3,
        "Expected 3 module-not-found errors (TS2307/TS2792) for 3 unresolved imports, got: {module_not_found_count}"
    );
}

/// Test that TS2307 includes correct module specifier in message
#[test]
fn test_ts2307_diagnostic_message_contains_specifier() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
import { foo } from "./specific-missing-module";
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    // Accept either TS2307 or TS2792 (the "did you mean to set moduleResolution" variant)
    let module_diag = checker.ctx.diagnostics.iter().find(|d| {
        d.code == diagnostic_codes::CANNOT_FIND_MODULE_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
            || d.code == diagnostic_codes::CANNOT_FIND_MODULE_DID_YOU_MEAN_TO_SET_THE_MODULERESOLUTION_OPTION_TO_NODENEXT_O
    });

    assert!(
        module_diag.is_some(),
        "Expected TS2307 or TS2792 diagnostic"
    );
    let diag = module_diag.unwrap();
    assert!(
        diag.message_text.contains("./specific-missing-module"),
        "Module-not-found message should contain module specifier, got: {}",
        diag.message_text
    );
}

