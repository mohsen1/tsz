//! Tests for global type error detection (TS2318, TS2583)
//!
//! These tests verify that missing global types emit appropriate errors:
//! - TS2318: Cannot find global type (for @noLib or pre-ES2015 types)
//! - TS2583: Cannot find name - suggests changing target library (for ES2015+ types)
//!
//! Note: These tests simulate missing lib.d.ts by not loading lib files.

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::test_fixtures::TestContext;

/// Helper function to create a checker without lib.d.ts and check source code.
/// This creates the checker with the parser's arena directly to ensure proper node resolution.
fn check_without_lib(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    check_without_lib_with_options(source, CheckerOptions::default())
}

/// Helper function to create a checker without lib.d.ts with custom options.
fn check_without_lib_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<crate::checker::types::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // No parse errors expected in these tests
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    // Don't merge any lib symbols - simulates @noLib

    let types = TypeInterner::new();

    let mut checker = CheckerState::new(
        parser.get_arena(), // Use parser's arena directly
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    // Don't set lib_contexts - no lib files loaded

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_missing_promise_emits_ts2583_without_lib() {
    // Without lib.d.ts, Promise should emit TS2583 (Cannot find name - change lib)
    let diagnostics = check_without_lib("const p = new Promise<void>();");

    // Should emit TS2583 for ES2015+ types like Promise when lib.d.ts is not loaded
    // TypeScript emits: "Cannot find name 'Promise'. Do you need to change your target library?"
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        !ts2583_errors.is_empty(),
        "Expected TS2583 error for Promise without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_missing_map_emits_ts2583_without_lib() {
    let diagnostics = check_without_lib("const m = new Map<string, number>();");

    // Should emit TS2583 for ES2015+ types like Map when lib.d.ts is not loaded
    // TypeScript emits: "Cannot find name 'Map'. Do you need to change your target library?"
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        !ts2583_errors.is_empty(),
        "Expected TS2583 error for Map without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_missing_set_emits_ts2583_without_lib() {
    let diagnostics = check_without_lib("const s = new Set<number>();");

    // Should emit TS2583 for ES2015+ types like Set when lib.d.ts is not loaded
    // TypeScript emits: "Cannot find name 'Set'. Do you need to change your target library?"
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        !ts2583_errors.is_empty(),
        "Expected TS2583 error for Set without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_missing_symbol_emits_ts2585_without_lib() {
    let diagnostics = check_without_lib(r#"const s = Symbol("foo");"#);

    // Should emit TS2585 for Symbol when lib.d.ts is not loaded
    // TypeScript emits: "'Symbol' only refers to a type, but is being used as a value here.
    // Do you need to change your target library?"
    let ts2585_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2585).collect();

    assert!(
        !ts2585_errors.is_empty(),
        "Expected TS2585 error for Symbol without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_missing_date_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib("const d = new Date();");

    // Should emit TS2304 for Date when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for Date without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_missing_regexp_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib(r#"const r = new RegExp("foo");"#);

    // Should emit TS2304 for RegExp when lib.d.ts is not loaded
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for RegExp without lib.d.ts"
    );
}

#[test]
fn test_promise_type_reference_emits_ts2583_without_lib() {
    let diagnostics = check_without_lib(
        r#"
function foo(): Promise<void> {
    return Promise.resolve();
}
"#,
    );

    // Should emit TS2583 for ES2015+ types like Promise when lib.d.ts is not loaded
    // TypeScript emits: "Cannot find name 'Promise'. Do you need to change your target library?"
    let ts2583_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2583).collect();

    assert!(
        !ts2583_errors.is_empty(),
        "Expected TS2583 errors for Promise without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_console_emits_ts2304_without_lib() {
    let diagnostics = check_without_lib(r#"console.log("hello");"#);

    // Should emit TS2584 for console when lib.d.ts is not loaded
    // (console is a known DOM global, so we suggest including 'dom' lib)
    let ts2584_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2584).collect();

    assert!(
        !ts2584_errors.is_empty(),
        "Expected TS2584 error for console without lib.d.ts, got: {:?}",
        diagnostics
    );
}

// Tests with lib.d.ts loaded - these should NOT emit errors

/// Helper function to create a checker WITH lib.d.ts and check source code.
/// This creates the checker with the parser's arena directly and loads lib files.
fn check_with_lib(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    use std::sync::Arc;

    let ctx = TestContext::new(); // This loads lib files

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // No parse errors expected in these tests
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(), // Use parser's arena directly
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    // Set lib contexts for global symbol resolution
    if !ctx.lib_files.is_empty() {
        let lib_contexts: Vec<crate::checker::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
#[ignore] // TODO: Fix this test
fn test_console_no_error_with_lib() {
    let diagnostics = check_with_lib(r#"console.log("hello");"#);

    // With lib.d.ts, console should not emit TS2304
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        ts2304_errors.is_empty(),
        "console should NOT emit TS2304 with lib.d.ts loaded, got: {:?}",
        ts2304_errors
    );
}

#[test]
fn test_array_no_error_with_lib() {
    let diagnostics = check_with_lib("const arr: Array<number> = [1, 2, 3];");

    // Array is a built-in type that should be available with lib.d.ts
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        ts2304_errors.is_empty(),
        "Array should NOT emit TS2304 with lib.d.ts loaded, got: {:?}",
        ts2304_errors
    );
}

#[test]
fn test_object_no_error_with_lib() {
    let diagnostics = check_with_lib("const obj: Object = {};");

    // Object is a built-in type that should be available with lib.d.ts
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    assert!(
        ts2304_errors.is_empty(),
        "Object should NOT emit TS2304 with lib.d.ts loaded, got: {:?}",
        ts2304_errors
    );
}

// Tests for decorator-related global types (TS2318 for TypedPropertyDescriptor)

#[test]
fn test_missing_typed_property_descriptor_with_decorators() {
    // When experimentalDecorators is enabled and a method has decorators,
    // TypedPropertyDescriptor must be available. If not, emit TS2318.
    let options = CheckerOptions {
        experimental_decorators: true,
        ..Default::default()
    };

    let diagnostics = check_without_lib_with_options(
        r#"
declare function dec(t: any, k: string, d: any): any;

class C {
    @dec
    method() {}
}
"#,
        options,
    );

    // Should emit TS2318 for TypedPropertyDescriptor when lib.d.ts is not loaded
    // and experimentalDecorators is enabled
    let ts2318_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2318).collect();

    eprintln!("All diagnostics: {:?}", diagnostics);
    eprintln!("TS2318 errors: {:?}", ts2318_errors);

    assert!(
        !ts2318_errors.is_empty(),
        "Expected TS2318 error for TypedPropertyDescriptor without lib.d.ts, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_no_ts2318_without_experimental_decorators() {
    // Without experimentalDecorators, decorators should not trigger TS2318
    let options = CheckerOptions {
        experimental_decorators: false,
        ..Default::default()
    };

    let diagnostics = check_without_lib_with_options(
        r#"
declare function dec(t: any, k: string, d: any): any;

class C {
    @dec
    method() {}
}
"#,
        options,
    );

    // Should NOT emit TS2318 when experimentalDecorators is disabled
    let ts2318_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2318).collect();

    assert!(
        ts2318_errors.is_empty(),
        "Should NOT emit TS2318 without experimentalDecorators, got: {:?}",
        ts2318_errors
    );
}

#[test]
fn test_decorator_ts2318_with_lib_contexts() {
    // Simulate the multi-file test: a.ts has core interfaces, b.ts has decorated class
    // This tests that lib_contexts don't wrongly suppress the TS2318 error
    use std::sync::Arc;
    use crate::checker::context::LibContext;

    let options = CheckerOptions {
        experimental_decorators: true,
        ..Default::default()
    };

    // Parse and bind a.ts (the "lib" file with core interfaces)
    let a_source = r#"
interface Object { }
interface Array<T> { }
interface String { }
interface Boolean { }
interface Number { }
interface Function { }
interface RegExp { }
interface IArguments { }
"#;
    let mut parser_a = ParserState::new("a.ts".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    // Parse and bind b.ts (the file with decorated class)
    let b_source = r#"
declare function dec(t: any, k: string, d: any): any;

class C {
    @dec
    method() {}
}
"#;
    let mut parser_b = ParserState::new("b.ts".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    // Create lib_contexts with BOTH a.ts and b.ts (same as server does)
    let arena_a = Arc::new(parser_a.into_arena());
    let binder_a = Arc::new(binder_a);
    let arena_b = Arc::new(parser_b.into_arena());
    let binder_b = Arc::new(binder_b);

    let lib_contexts = vec![
        LibContext {
            arena: arena_a.clone(),
            binder: binder_a.clone(),
        },
        LibContext {
            arena: arena_b.clone(),
            binder: binder_b.clone(),
        },
    ];

    // Check b.ts with lib_contexts set (including both a.ts and b.ts)
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        &arena_b,
        &binder_b,
        &types,
        "b.ts".to_string(),
        options,
    );
    checker.ctx.set_lib_contexts(lib_contexts);

    checker.check_source_file(root_b);
    let diagnostics = checker.ctx.diagnostics.clone();

    // Should emit TS2318 for TypedPropertyDescriptor
    let ts2318_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2318 && d.message_text.contains("TypedPropertyDescriptor"))
        .collect();

    eprintln!("All diagnostics for b.ts: {:?}", diagnostics);
    eprintln!("TS2318 for TypedPropertyDescriptor: {:?}", ts2318_errors);

    assert!(
        !ts2318_errors.is_empty(),
        "Expected TS2318 for TypedPropertyDescriptor even with lib_contexts, got: {:?}",
        diagnostics
    );
}
