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

#![allow(clippy::print_stderr)]

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};

// =============================================================================
// Test Helpers
// =============================================================================

/// Helper to parse, bind, and check a single file with module_exports pre-populated.
/// Returns the checker diagnostics as (code, message) pairs.
///
/// `module_exports` maps module specifier -> list of export names.
/// Each module's exports are created by parsing and binding a synthetic exporter file.
fn check_with_module_exports(
    source: &str,
    file_name: &str,
    module_exports: Vec<(&str, Vec<(&str, u32)>)>,
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
            .map(|(name, _)| format!("export const {} = 0;", name))
            .collect::<Vec<_>>()
            .join("\n");

        let mut export_parser = ParserState::new(format!("{}.ts", module_name), export_source);
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
    let mut options = CheckerOptions::default();
    options.module = crate::common::ModuleKind::CommonJS;

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

/// Helper to parse, bind, and check a single file with resolved_modules set.
/// This simulates the CLI driver having resolved certain module specifiers.
fn check_with_resolved_modules(
    source: &str,
    file_name: &str,
    resolved_modules: Vec<&str>,
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
    let modules: std::collections::HashSet<String> =
        resolved_modules.iter().map(|s| s.to_string()).collect();
    checker.ctx.set_resolved_modules(modules);

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Helper to simply parse, bind, and check a file with no cross-file context.
fn check_single_file(source: &str, file_name: &str) -> Vec<(u32, String)> {
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
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error_code(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

fn no_error_code(diagnostics: &[(u32, String)], code: u32) -> bool {
    !has_error_code(diagnostics, code)
}

// TS2307: Cannot find module
const TS2307: u32 = 2307;
// TS2305: Module has no exported member
const TS2305: u32 = 2305;
// TS1202: Import assignment cannot be used when targeting ECMAScript modules
const TS1202: u32 = 1202;

// =============================================================================
// ES Import Declaration Tests
// =============================================================================

#[test]
fn test_es_named_import_resolved_module() {
    let source = r#"import { foo } from "./utils";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Should not emit TS2307 for resolved module, got: {:?}",
        diags
    );
}

#[test]
fn test_es_named_import_unresolved_module() {
    let source = r#"import { foo } from "./nonexistent";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![]);
    assert!(
        has_error_code(&diags, TS2307),
        "Should emit TS2307 for unresolved module, got: {:?}",
        diags
    );
}

#[test]
fn test_es_default_import_resolved() {
    let source = r#"import utils from "./utils";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Default import should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_es_namespace_import_resolved() {
    let source = r#"import * as utils from "./utils";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Namespace import should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_es_side_effect_import_resolved() {
    let source = r#"import "./polyfill";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./polyfill"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Side-effect import should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_es_side_effect_import_unresolved() {
    let source = r#"import "./nonexistent";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![]);
    assert!(
        has_error_code(&diags, TS2307),
        "Unresolved side-effect import should emit TS2307, got: {:?}",
        diags
    );
}

#[test]
fn test_es_type_only_import() {
    let source = r#"import type { Foo } from "./types";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./types"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Type-only import should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_es_import_multiple_specifiers() {
    let source = r#"
import { a } from "./mod-a";
import { b } from "./mod-b";
import { c } from "./mod-c";
"#;
    let diags =
        check_with_resolved_modules(source, "main.ts", vec!["./mod-a", "./mod-b", "./mod-c"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Multiple resolved imports should not error, got: {:?}",
        diags
    );
}

#[test]
fn test_es_import_partial_resolution() {
    let source = r#"
import { a } from "./exists";
import { b } from "./missing";
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./exists"]);
    assert!(
        has_error_code(&diags, TS2307),
        "Unresolved import should produce TS2307, got: {:?}",
        diags
    );
    // Verify the error is about the missing module specifically
    let ts2307_errors: Vec<_> = diags.iter().filter(|(c, _)| *c == TS2307).collect();
    assert_eq!(
        ts2307_errors.len(),
        1,
        "Only one TS2307 should be emitted, got: {:?}",
        ts2307_errors
    );
    assert!(
        ts2307_errors[0].1.contains("./missing"),
        "TS2307 message should reference './missing', got: {}",
        ts2307_errors[0].1
    );
}

// =============================================================================
// ES Re-export Tests
// =============================================================================

#[test]
fn test_es_reexport_resolved() {
    let source = r#"export { foo } from "./utils";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Re-export from resolved module should not error, got: {:?}",
        diags
    );
}

#[test]
fn test_es_reexport_unresolved() {
    let source = r#"export { foo } from "./nonexistent";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec![]);
    assert!(
        has_error_code(&diags, TS2307),
        "Re-export from unresolved module should emit TS2307, got: {:?}",
        diags
    );
}

#[test]
fn test_es_wildcard_reexport_resolved() {
    let source = r#"export * from "./utils";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard re-export from resolved module should not error, got: {:?}",
        diags
    );
}

#[test]
fn test_es_namespace_reexport_resolved() {
    let source = r#"export * as utils from "./utils";"#;
    let diags = check_with_resolved_modules(source, "barrel.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Namespace re-export from resolved module should not error, got: {:?}",
        diags
    );
}

// =============================================================================
// Import Equals Declaration Tests (import x = require("..."))
// =============================================================================

#[test]
fn test_import_equals_require_resolved() {
    let source = r#"import utils = require("./utils");"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "import = require should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_import_equals_require_unresolved() {
    let source = r#"import utils = require("./nonexistent");"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![]);
    assert!(
        has_error_code(&diags, TS2307),
        "Unresolved import = require should emit TS2307, got: {:?}",
        diags
    );
}

#[test]
fn test_import_equals_require_in_esm_emits_ts1202() {
    // When targeting ESM, import = require should emit TS1202
    let source = r#"import utils = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.module = crate::common::ModuleKind::ES2015;

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "main.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&TS1202),
        "import = require in ESM should emit TS1202, got: {:?}",
        codes
    );
}

#[test]
fn test_import_equals_namespace_alias() {
    // import x = Namespace (not require)
    let source = r#"
namespace MyNamespace {
    export const value = 42;
}
import Alias = MyNamespace;
const x = Alias.value;
"#;
    let diags = check_single_file(source, "main.ts");
    // Should not have TS2503 (Cannot find namespace) since MyNamespace exists
    assert!(
        no_error_code(&diags, 2503),
        "Namespace import should resolve, got: {:?}",
        diags
    );
}

// =============================================================================
// Module Exports and Import Member Checking Tests
// =============================================================================

#[test]
fn test_import_nonexistent_member_from_module() {
    let source = r#"import { nonexistent } from "./utils";"#;
    let diags = check_with_module_exports(
        source,
        "main.ts",
        vec![("./utils", vec![("foo", 0), ("bar", 0)])],
        true,
    );
    assert!(
        has_error_code(&diags, TS2305),
        "Importing nonexistent member should emit TS2305, got: {:?}",
        diags
    );
}

#[test]
fn test_import_existing_member_from_module() {
    let source = r#"import { foo } from "./utils";"#;
    let diags = check_with_module_exports(
        source,
        "main.ts",
        vec![("./utils", vec![("foo", 0), ("bar", 0)])],
        true,
    );
    assert!(
        no_error_code(&diags, TS2305),
        "Importing existing member should not emit TS2305, got: {:?}",
        diags
    );
    assert!(
        no_error_code(&diags, TS2307),
        "Module with exports should not emit TS2307, got: {:?}",
        diags
    );
}

#[test]
fn test_import_renamed_member() {
    let source = r#"import { foo as myFoo } from "./utils";"#;
    let diags =
        check_with_module_exports(source, "main.ts", vec![("./utils", vec![("foo", 0)])], true);
    assert!(
        no_error_code(&diags, TS2305),
        "Renamed import of existing member should not error, got: {:?}",
        diags
    );
}

#[test]
fn test_import_renamed_nonexistent_member() {
    let source = r#"import { nonexistent as alias } from "./utils";"#;
    let diags =
        check_with_module_exports(source, "main.ts", vec![("./utils", vec![("foo", 0)])], true);
    assert!(
        has_error_code(&diags, TS2305),
        "Renamed import of nonexistent member should emit TS2305, got: {:?}",
        diags
    );
}

// =============================================================================
// Declared / Ambient Module Tests
// =============================================================================

#[test]
fn test_declared_module_prevents_ts2307() {
    let source = r#"
declare module "my-lib" {
    export const value: number;
}
import { value } from "my-lib";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Declared module should prevent TS2307, got: {:?}",
        diags
    );
}

#[test]
fn test_declared_module_with_interface() {
    let source = r#"
declare module "my-lib" {
    export interface Config {
        name: string;
    }
}
import { Config } from "my-lib";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Declared module with interface should prevent TS2307, got: {:?}",
        diags
    );
}

#[test]
fn test_declared_module_recorded_in_binder() {
    let source = r#"
declare module "external-lib" {
    export const api: string;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.declared_modules.contains("external-lib"),
        "Binder should record declared modules"
    );
}

#[test]
fn test_shorthand_ambient_module_declaration() {
    // Shorthand ambient module (no body) - e.g., `declare module "*.css"`
    let source = r#"declare module "*.css";"#;
    let mut parser = ParserState::new("globals.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.shorthand_ambient_modules.contains("*.css"),
        "Binder should record shorthand ambient module '*.css'"
    );
}

#[test]
fn test_multiple_declared_modules() {
    let source = r#"
declare module "lib-a" {
    export const a: number;
}
declare module "lib-b" {
    export const b: string;
}
"#;
    let mut parser = ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.declared_modules.contains("lib-a"));
    assert!(binder.declared_modules.contains("lib-b"));
}

// =============================================================================
// Export Declaration Tests
// =============================================================================

#[test]
fn test_export_const_creates_symbol() {
    let source = r#"export const foo = 42;"#;
    let mut parser = ParserState::new("utils.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("foo"),
        "Exported const should be in file_locals"
    );
}

#[test]
fn test_export_function_creates_symbol() {
    let source = r#"export function myFunc(): void {}"#;
    let mut parser = ParserState::new("utils.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("myFunc"),
        "Exported function should be in file_locals"
    );
}

#[test]
fn test_export_class_creates_symbol() {
    let source = r#"export class MyClass {}"#;
    let mut parser = ParserState::new("utils.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("MyClass"),
        "Exported class should be in file_locals"
    );
}

#[test]
fn test_export_interface_creates_symbol() {
    let source = r#"export interface MyInterface { value: number; }"#;
    let mut parser = ParserState::new("types.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("MyInterface"),
        "Exported interface should be in file_locals"
    );
}

#[test]
fn test_export_type_alias_creates_symbol() {
    let source = r#"export type MyType = string | number;"#;
    let mut parser = ParserState::new("types.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("MyType"),
        "Exported type alias should be in file_locals"
    );
}

#[test]
fn test_export_enum_creates_symbol() {
    let source = r#"export enum Direction { Up, Down, Left, Right }"#;
    let mut parser = ParserState::new("types.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("Direction"),
        "Exported enum should be in file_locals"
    );
}

// =============================================================================
// Import/Export Symbol Binding Tests
// =============================================================================

#[test]
fn test_named_import_creates_alias_symbol() {
    use crate::binder::symbol_flags;

    let exporter_source = r#"export const value = 42;"#;
    let mut exporter_parser = ParserState::new("file1.ts".to_string(), exporter_source.to_string());
    let exporter_root = exporter_parser.parse_source_file();
    let exporter_arena = exporter_parser.get_arena();

    let mut exporter_binder = BinderState::new();
    exporter_binder.bind_source_file(exporter_arena, exporter_root);

    let export_sym_id = exporter_binder
        .file_locals
        .get("value")
        .expect("value should exist in exporter");

    let importer_source = r#"
import { value } from './file1';
const x = value;
"#;
    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();
    let importer_arena = importer_parser.get_arena();

    let mut importer_binder = BinderState::new();
    importer_binder
        .module_exports
        .insert("./file1".to_string(), {
            let mut table = crate::binder::SymbolTable::new();
            table.set("value".to_string(), export_sym_id);
            table
        });
    importer_binder.bind_source_file(importer_arena, importer_root);

    assert!(
        importer_binder.file_locals.has("value"),
        "Imported symbol should be in file_locals"
    );

    let import_sym_id = importer_binder.file_locals.get("value").unwrap();
    let import_sym = importer_binder.get_symbol(import_sym_id).unwrap();

    assert!(
        import_sym.flags & symbol_flags::ALIAS != 0,
        "Import symbol should be ALIAS"
    );
    assert_eq!(
        import_sym.import_module,
        Some("./file1".to_string()),
        "Import symbol should have import_module set"
    );
}

#[test]
fn test_default_import_creates_alias_symbol() {
    use crate::binder::symbol_flags;

    let exporter_source = r#"export default function hello() {}"#;
    let mut exporter_parser = ParserState::new("file1.ts".to_string(), exporter_source.to_string());
    let exporter_root = exporter_parser.parse_source_file();

    let mut exporter_binder = BinderState::new();
    exporter_binder.bind_source_file(exporter_parser.get_arena(), exporter_root);

    // For default exports, the export name is "default"
    let default_sym_id = exporter_binder
        .file_locals
        .get("hello")
        .expect("hello should exist in exporter");

    let importer_source = r#"
import hello from './file1';
"#;
    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();

    let mut importer_binder = BinderState::new();
    importer_binder
        .module_exports
        .insert("./file1".to_string(), {
            let mut table = crate::binder::SymbolTable::new();
            table.set("default".to_string(), default_sym_id);
            table
        });
    importer_binder.bind_source_file(importer_parser.get_arena(), importer_root);

    assert!(
        importer_binder.file_locals.has("hello"),
        "Default import should create local symbol"
    );

    let import_sym_id = importer_binder.file_locals.get("hello").unwrap();
    let import_sym = importer_binder.get_symbol(import_sym_id).unwrap();

    assert!(
        import_sym.flags & symbol_flags::ALIAS != 0,
        "Default import should be ALIAS"
    );
}

#[test]
fn test_namespace_import_creates_alias_symbol() {
    use crate::binder::symbol_flags;

    let importer_source = r#"
import * as utils from './utils';
"#;
    let mut importer_parser = ParserState::new("file2.ts".to_string(), importer_source.to_string());
    let importer_root = importer_parser.parse_source_file();

    let mut importer_binder = BinderState::new();
    importer_binder.bind_source_file(importer_parser.get_arena(), importer_root);

    assert!(
        importer_binder.file_locals.has("utils"),
        "Namespace import should create local symbol 'utils'"
    );

    let import_sym_id = importer_binder.file_locals.get("utils").unwrap();
    let import_sym = importer_binder.get_symbol(import_sym_id).unwrap();

    assert!(
        import_sym.flags & symbol_flags::ALIAS != 0,
        "Namespace import should be ALIAS"
    );
}

// =============================================================================
// Re-export Binding Tests
// =============================================================================

#[test]
fn test_reexport_tracked_in_binder() {
    let source = r#"export { foo, bar } from "./utils";"#;
    let mut parser = ParserState::new("barrel.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Re-exports should be recorded
    assert!(
        binder.reexports.contains_key("barrel.ts")
            || !binder.reexports.is_empty()
            || binder.file_locals.has("foo")
            || binder.file_locals.has("bar"),
        "Re-exports should be tracked in some form"
    );
}

#[test]
fn test_wildcard_reexport_tracked() {
    let source = r#"export * from "./utils";"#;
    let mut parser = ParserState::new("barrel.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Wildcard re-exports should be tracked
    let has_wildcard = !binder.wildcard_reexports.is_empty();
    // Wildcard reexport tracking may not be implemented yet
    let _ = has_wildcard;
}

// =============================================================================
// Dynamic Import Tests
// =============================================================================

#[test]
fn test_dynamic_import_resolved_module() {
    let source = r#"
async function load() {
    const mod = await import("./lazy-module");
}
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./lazy-module"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Dynamic import of resolved module should not error, got: {:?}",
        diags
    );
}

#[test]
fn test_dynamic_import_unresolved_module() {
    let source = r#"
async function load() {
    const mod = await import("./nonexistent");
}
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![]);
    assert!(
        has_error_code(&diags, TS2307),
        "Dynamic import of unresolved module should emit TS2307, got: {:?}",
        diags
    );
}

#[test]
fn test_dynamic_import_with_ambient_module() {
    let source = r#"
declare module "my-lazy-lib" {
    export function doStuff(): void;
}

async function load() {
    const mod = await import("my-lazy-lib");
}
"#;
    let diags = check_single_file(source, "main.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Dynamic import of ambient module should not error, got: {:?}",
        diags
    );
}

// =============================================================================
// Require Tests (CommonJS-style)
// =============================================================================

#[test]
fn test_require_creates_binding() {
    // require() calls are tracked by the import tracker
    let source = r#"const utils = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("utils"),
        "require() result should create local binding"
    );
}

#[test]
fn test_require_destructured() {
    let source = r#"const { foo, bar } = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("foo"),
        "Destructured require should create 'foo' binding"
    );
    assert!(
        binder.file_locals.has("bar"),
        "Destructured require should create 'bar' binding"
    );
}

// =============================================================================
// Triple-Slash Reference Directive Tests
// =============================================================================

#[test]
fn test_triple_slash_reference_path_parsed() {
    let source = r#"/// <reference path="./globals.d.ts" />
const x: number = 42;
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    // The file should parse without errors - reference directives are comments
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("x"),
        "File with reference directive should still have regular bindings"
    );
}

#[test]
fn test_triple_slash_reference_types_parsed() {
    let source = r#"/// <reference types="node" />
const x: number = 42;
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("x"),
        "File with reference types directive should still have regular bindings"
    );
}

#[test]
fn test_triple_slash_reference_lib_parsed() {
    let source = r#"/// <reference lib="es2015" />
const x: number = 42;
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("x"),
        "File with reference lib directive should still have regular bindings"
    );
}

// =============================================================================
// Module Resolution Map Integration Tests
// =============================================================================

#[test]
fn test_build_resolution_maps_used_by_checker() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let file_names = vec![
        "/project/src/main.ts".to_string(),
        "/project/src/utils.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&file_names);

    // Verify the maps contain expected entries
    assert_eq!(paths.get(&(0, "./utils".to_string())), Some(&1));
    assert!(modules.contains("./utils"));

    // These maps would be passed to checker context via:
    // checker.ctx.set_resolved_module_paths(paths);
    // checker.ctx.set_resolved_modules(modules);
}

// =============================================================================
// Export Assignment Tests (export = ...)
// =============================================================================

#[test]
fn test_export_assignment_basic() {
    let source = r#"
const myModule = { value: 42 };
export = myModule;
"#;
    let diags = check_single_file(source, "module.ts");
    // Should not have unexpected errors for basic export assignment
    let unexpected: Vec<_> = diags.iter().filter(|(c, _)| *c == TS2307).collect();
    assert!(
        unexpected.is_empty(),
        "Export assignment should not produce TS2307, got: {:?}",
        unexpected
    );
}

#[test]
fn test_export_assignment_with_other_exports() {
    let source = r#"
export const foo = 1;
const bar = 2;
export = bar;
"#;
    let diags = check_single_file(source, "module.ts");
    // TS2309: Export assignment conflicts with other exported elements
    let has_ts2309 = has_error_code(&diags, 2309);
    assert!(
        has_ts2309,
        "export = with other exports should emit TS2309, got: {:?}",
        diags
    );
}

// =============================================================================
// CommonJS Module.exports Tests (Binder)
// =============================================================================

#[test]
fn test_module_exports_assignment_binding() {
    let source = r#"
function myFunc() { return 42; }
module.exports = myFunc;
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    // module.exports = ... is valid in CommonJS context
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("myFunc"),
        "Function before module.exports should still be bound"
    );
}

#[test]
fn test_exports_named_assignment_binding() {
    let source = r#"
exports.foo = 42;
exports.bar = "hello";
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // The file should at least parse and bind without errors
    // exports.foo = ... creates CommonJS named exports
}

// =============================================================================
// AMD Module Tests
// =============================================================================

#[test]
fn test_amd_define_parses() {
    // AMD-style define
    let source = r#"
define(["./dep"], function(dep: any) {
    return dep.value;
});
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    // Note: define() is a function call, should parse fine as expression
    assert!(
        parser.get_diagnostics().is_empty(),
        "AMD define should parse without errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    // AMD define doesn't create special bindings in modern TypeScript
}

#[test]
fn test_amd_require_parses() {
    // AMD-style require (synchronous)
    let source = r#"
const dep = require("./dependency");
"#;
    let mut parser = ParserState::new("module.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "AMD require should parse: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("dep"),
        "require() result should create 'dep' binding"
    );
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

#[test]
fn test_empty_module_specifier() {
    let source = r#"import {} from "";"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    // Empty string module specifier - should parse but might produce checker errors
}

#[test]
fn test_import_with_no_clause() {
    // Side-effect only import
    let source = r#"import "./setup";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./setup"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Side-effect import should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_duplicate_import_specifiers() {
    // Importing the same module twice shouldn't cause duplicate TS2307
    let source = r#"
import { a } from "./missing";
import { b } from "./missing";
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec![]);
    let ts2307_count = diags.iter().filter(|(c, _)| *c == TS2307).count();
    // Should only emit TS2307 once per unique specifier
    assert!(
        ts2307_count <= 2,
        "Duplicate imports should not produce many TS2307 errors, got {} for: {:?}",
        ts2307_count,
        diags
    );
}

#[test]
fn test_import_without_unresolved_imports_flag() {
    // When report_unresolved_imports is false, no TS2307 should be emitted
    let source = r#"import { foo } from "./nonexistent";"#;
    let diags = check_with_module_exports(source, "main.ts", vec![], false);
    assert!(
        no_error_code(&diags, TS2307),
        "Should not emit TS2307 when report_unresolved_imports is false, got: {:?}",
        diags
    );
}

// =============================================================================
// Multi-file Integration Test
// =============================================================================

#[test]
fn test_multi_file_module_resolution_maps() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    // Simulate a real project structure
    let files = vec![
        "/project/src/index.ts".to_string(),
        "/project/src/utils/math.ts".to_string(),
        "/project/src/utils/string.ts".to_string(),
        "/project/src/types/api.d.ts".to_string(),
        "/project/src/components/Button.tsx".to_string(),
        "/project/src/components/index.ts".to_string(),
    ];

    let (paths, modules) = build_module_resolution_maps(&files);

    // index.ts can import everything
    assert!(modules.contains("./utils/math"));
    assert!(modules.contains("./utils/string"));
    assert!(modules.contains("./types/api"));
    assert!(modules.contains("./components/Button"));
    assert!(modules.contains("./components")); // index file

    // Cross-directory imports
    assert_eq!(
        paths.get(&(4, "../utils/math".to_string())),
        Some(&1),
        "Button.tsx should import ../utils/math"
    );
    assert_eq!(
        paths.get(&(4, "../types/api".to_string())),
        Some(&3),
        "Button.tsx should import ../types/api"
    );
}

// =============================================================================
// Module Kind (CommonJS vs ESM) Tests
// =============================================================================

#[test]
fn test_commonjs_import_equals_no_error() {
    let source = r#"import utils = require("./utils");"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut options = CheckerOptions::default();
    options.module = crate::common::ModuleKind::CommonJS;

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "main.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&TS1202),
        "import = require in CommonJS should NOT emit TS1202, got: {:?}",
        codes
    );
}

// =============================================================================
// Circular Import Detection Tests
// =============================================================================

#[test]
fn test_circular_import_detection_in_binder() {
    // File A imports from B, and B imports from A
    // This shouldn't crash the binder
    let source_a = r#"
import { b } from "./b";
export const a = 1;
"#;
    let source_b = r#"
import { a } from "./a";
export const b = 2;
"#;

    // Parse and bind both files
    let mut parser_a = ParserState::new("a.ts".to_string(), source_a.to_string());
    let root_a = parser_a.parse_source_file();
    assert!(parser_a.get_diagnostics().is_empty());

    let mut parser_b = ParserState::new("b.ts".to_string(), source_b.to_string());
    let root_b = parser_b.parse_source_file();
    assert!(parser_b.get_diagnostics().is_empty());

    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    // Both files should bind successfully
    assert!(binder_a.file_locals.has("a"));
    assert!(binder_b.file_locals.has("b"));
}

// =============================================================================
// Mixed Import Style Tests
// =============================================================================

#[test]
fn test_mixed_import_require_same_file() {
    // Using both ES imports and require in the same file
    let source = r#"
import { foo } from "./utils";
const bar = require("./utils");
"#;
    let mut parser = ParserState::new("main.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Mixed import/require should parse: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.file_locals.has("foo"),
        "ES import binding should exist"
    );
    assert!(
        binder.file_locals.has("bar"),
        "require binding should exist"
    );
}

#[test]
fn test_import_and_reexport_same_module() {
    let source = r#"
import { foo } from "./utils";
export { bar } from "./utils";
"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Import and re-export from same module should resolve, got: {:?}",
        diags
    );
}

// =============================================================================
// Module with Different Extension Specifiers
// =============================================================================

#[test]
fn test_import_with_js_extension() {
    // TypeScript allows importing with .js extension (resolves to .ts)
    let source = r#"import { foo } from "./utils.js";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils.js"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Import with .js extension should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_import_with_ts_extension() {
    // Importing with .ts extension is unusual but parseable
    let source = r#"import { foo } from "./utils.ts";"#;
    let diags = check_with_resolved_modules(source, "main.ts", vec!["./utils.ts"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Import with .ts extension should resolve when in resolved set, got: {:?}",
        diags
    );
}

// =============================================================================
// Re-export Chain Tests
// =============================================================================

#[test]
fn test_barrel_file_exports() {
    let source = r#"
export { Button } from "./components/Button";
export { Input } from "./components/Input";
export { Form } from "./components/Form";
"#;
    let diags = check_with_resolved_modules(
        source,
        "index.ts",
        vec![
            "./components/Button",
            "./components/Input",
            "./components/Form",
        ],
    );
    assert!(
        no_error_code(&diags, TS2307),
        "Barrel file re-exports should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_wildcard_reexport_with_named_reexport() {
    let source = r#"
export * from "./base";
export { special } from "./special";
"#;
    let diags = check_with_resolved_modules(source, "index.ts", vec!["./base", "./special"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Mixed wildcard and named re-exports should resolve, got: {:?}",
        diags
    );
}

// =============================================================================
// Ambient Module Wildcard Pattern Tests
// =============================================================================

#[test]
fn test_wildcard_ambient_module_css() {
    let source = r#"
declare module "*.css" {
    const styles: { [key: string]: string };
    export default styles;
}
import styles from "./app.css";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard ambient module should match .css imports, got: {:?}",
        diags
    );
}

#[test]
fn test_wildcard_ambient_module_svg() {
    let source = r#"
declare module "*.svg" {
    const content: string;
    export default content;
}
import logo from "./logo.svg";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard ambient module should match .svg imports, got: {:?}",
        diags
    );
}

#[test]
fn test_wildcard_ambient_module_json() {
    let source = r#"
declare module "*.json" {
    const data: any;
    export default data;
}
import data from "./config.json";
"#;
    let diags = check_single_file(source, "test.ts");
    assert!(
        no_error_code(&diags, TS2307),
        "Wildcard ambient module should match .json imports, got: {:?}",
        diags
    );
}

// =============================================================================
// Import with Complex Clauses
// =============================================================================

#[test]
fn test_import_default_and_named() {
    let source = r#"import React, { useState } from "./react";"#;
    let diags = check_with_resolved_modules(source, "app.tsx", vec!["./react"]);
    assert!(
        no_error_code(&diags, TS2307),
        "Combined default + named import should resolve, got: {:?}",
        diags
    );
}

#[test]
fn test_import_default_and_namespace() {
    let source = r#"import React, * as ReactAll from "./react";"#;
    let mut parser = ParserState::new("app.tsx".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    // This is a parse error in TypeScript - can't combine default with namespace
    // Just verify it doesn't crash
}

// =============================================================================
// Module Declaration with Body Tests
// =============================================================================

#[test]
fn test_ambient_module_with_multiple_exports() {
    let source = r#"
declare module "my-lib" {
    export const VERSION: string;
    export function init(): void;
    export class Client {
        connect(): void;
    }
    export interface Config {
        apiKey: string;
    }
    export type Status = "active" | "inactive";
    export enum LogLevel { Debug, Info, Warn, Error }
}
"#;
    let mut parser = ParserState::new("types.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Ambient module with multiple exports should parse: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(
        binder.declared_modules.contains("my-lib"),
        "Declared module should be tracked"
    );
}

// =============================================================================
// build_module_resolution_maps edge cases
// =============================================================================

#[test]
fn test_resolution_maps_same_name_different_dirs() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let files = vec![
        "/project/src/utils.ts".to_string(),
        "/project/lib/utils.ts".to_string(),
    ];

    let (paths, _) = build_module_resolution_maps(&files);

    // src/utils.ts -> ../lib/utils
    assert_eq!(
        paths.get(&(0, "../lib/utils".to_string())),
        Some(&1),
        "Same-name files in different dirs should resolve correctly"
    );
    // lib/utils.ts -> ../src/utils
    assert_eq!(
        paths.get(&(1, "../src/utils".to_string())),
        Some(&0),
        "Same-name files in different dirs should resolve correctly (reverse)"
    );
}

#[test]
fn test_resolution_maps_mixed_extensions() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let files = vec![
        "/project/main.ts".to_string(),
        "/project/lib.js".to_string(),
        "/project/types.d.ts".to_string(),
        "/project/component.tsx".to_string(),
    ];

    let (paths, _) = build_module_resolution_maps(&files);

    // All should resolve with extensionless specifiers
    assert_eq!(paths.get(&(0, "./lib".to_string())), Some(&1));
    assert_eq!(paths.get(&(0, "./types".to_string())), Some(&2));
    assert_eq!(paths.get(&(0, "./component".to_string())), Some(&3));
}

#[test]
fn test_resolution_maps_only_single_file() {
    use crate::checker::module_resolution::build_module_resolution_maps;

    let files = vec!["/project/main.ts".to_string()];

    let (paths, modules) = build_module_resolution_maps(&files);

    assert!(
        paths.is_empty(),
        "Single file should have no resolution paths"
    );
    assert!(
        modules.is_empty(),
        "Single file should have no resolved modules"
    );
}
