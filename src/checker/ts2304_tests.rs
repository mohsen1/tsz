//! Tests for TS2304 emission ("Cannot find name")
//!
//! These tests verify that:
//! 1. TS2304 is emitted when referencing undefined names
//! 2. TS2304 is NOT emitted when lib.d.ts is loaded and provides the name
//! 3. The "Any poisoning" effect is eliminated

use crate::test_fixtures::TestContext;

#[test]
fn test_ts2304_emitted_for_undefined_name() {
    // Create a test context WITHOUT lib.d.ts
    let ctx = TestContext::new_without_lib();
    let mut checker = ctx.checker();

    // Parse code that references an undefined name
    let source = r#"
    const x = undefinedName;  // Should emit TS2304
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder.bind_source_file(parser.get_arena(), root);

    // This should emit TS2304 for undefinedName
    checker.check_source_file(root);

    // Verify TS2304 was emitted
    let diagnostics = &checker.ctx.diagnostics;
    let ts2304_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();

    assert!(!ts2304_errors.is_empty(), "Expected TS2304 error for undefinedName");
}

#[test]
fn test_ts2304_not_emitted_for_lib_globals_with_lib() {
    // Create a test context WITH lib.d.ts
    let ctx = TestContext::new();
    let mut checker = ctx.checker();

    // Parse code that references console (defined in lib.d.ts)
    let source = r#"
    console.log("hello");  // Should NOT emit TS2304
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Bind with lib symbols merged
    ctx.binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    checker.check_source_file(root);

    // Verify TS2304 was NOT emitted for console
    let diagnostics = &checker.ctx.diagnostics;
    let ts2304_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();

    assert!(ts2304_errors.is_empty(), "Should NOT have TS2304 error for console when lib.d.ts is loaded");
}

#[test]
fn test_ts2304_emitted_for_console_without_lib() {
    // Create a test context WITHOUT lib.d.ts
    let ctx = TestContext::new_without_lib();
    let mut checker = ctx.checker();

    // Parse code that references console (NOT defined without lib.d.ts)
    let source = r#"
    console.log("hello");  // Should emit TS2304 when lib.d.ts is not loaded
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder.bind_source_file(parser.get_arena(), root);

    checker.check_source_file(root);

    // Verify TS2304 was emitted for console
    let diagnostics = &checker.ctx.diagnostics;
    let ts2304_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();

    assert!(!ts2304_errors.is_empty(), "Expected TS2304 error for console when lib.d.ts is not loaded");
}

#[test]
fn test_any_poisoning_eliminated() {
    // Create a test context WITHOUT lib.d.ts
    let ctx = TestContext::new_without_lib();
    let mut checker = ctx.checker();

    // Parse code that would have "Any poisoning" effect
    // If Array returned Any, this would NOT emit an error
    // But since Array should emit TS2304, we should get the error
    let source = r#"
    const arr: string = new Array();  // TS2304 for Array, then TS2322 for type mismatch
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder.bind_source_file(parser.get_arena(), root);

    checker.check_source_file(root);

    // Verify TS2304 was emitted for Array
    let diagnostics = &checker.ctx.diagnostics;
    let ts2304_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();

    assert!(!ts2304_errors.is_empty(), "Expected TS2304 error for Array when lib.d.ts is not loaded (any poisoning eliminated)");
}

#[test]
fn test_any_poisoning_fix_console_returns_void_not_any() {
    // Create a test context WITH lib.d.ts
    let ctx = TestContext::new();
    let mut checker = ctx.checker();

    // Parse code that tests console.log returns void, not any
    // This verifies the "Any poisoning" fix - console should have proper type
    let source = r#"
    const x: string = console.log("test");  // Should emit TS2322: void is not assignable to string
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Bind with lib symbols merged
    ctx.binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    checker.check_source_file(root);

    // Verify TS2322 was emitted (type mismatch), not TS2304
    let diagnostics = &checker.ctx.diagnostics;
    let ts2322_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    // The fix should cause TS2322 (type mismatch) because console.log returns void
    // Before the fix, console was "any" and this would not error
    assert!(!ts2322_errors.is_empty(), "Expected TS2322 error (void not assignable to string) - proves console is not 'any'");
}

#[test]
fn test_any_poisoning_fix_array_typed_correctly() {
    // Create a test context WITH lib.d.ts
    let ctx = TestContext::new();
    let mut checker = ctx.checker();

    // Parse code that tests Array is properly typed
    let source = r#"
    const arr: string = new Array<number>();  // Should emit TS2322: number[] is not assignable to string
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    ctx.binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    checker.check_source_file(root);

    // Verify TS2322 was emitted
    let diagnostics = &checker.ctx.diagnostics;
    let ts2322_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert!(!ts2322_errors.is_empty(), "Expected TS2322 error - proves Array is properly typed, not 'any'");
}

#[test]
fn test_any_poisoning_fix_promise_typed_correctly() {
    // Create a test context WITH lib.d.ts
    let ctx = TestContext::new();
    let mut checker = ctx.checker();

    // Parse code that tests Promise is properly typed
    let source = r#"
    const p: Promise<string> = Promise.resolve(42);  // Should emit TS2322: Promise<number> not assignable to Promise<string>
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    ctx.binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    checker.check_source_file(root);

    // Verify TS2322 was emitted
    let diagnostics = &checker.ctx.diagnostics;
    let ts2322_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert!(!ts2322_errors.is_empty(), "Expected TS2322 error - proves Promise is properly typed, not 'any'");
}

#[test]
fn test_any_poisoning_fix_math_returns_number() {
    // Create a test context WITH lib.d.ts
    let ctx = TestContext::new();
    let mut checker = ctx.checker();

    // Parse code that tests Math.abs returns number
    let source = r#"
    const m: string = Math.abs(-5);  // Should emit TS2322: number is not assignable to string
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    ctx.binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    checker.check_source_file(root);

    // Verify TS2322 was emitted
    let diagnostics = &checker.ctx.diagnostics;
    let ts2322_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert!(!ts2322_errors.is_empty(), "Expected TS2322 error - proves Math.abs returns number, not 'any'");
}

#[test]
fn test_any_poisoning_fix_multiple_globals() {
    // Create a test context WITH lib.d.ts
    let ctx = TestContext::new();
    let mut checker = ctx.checker();

    // Parse code that tests multiple global symbols are properly typed
    let source = r#"
    const a: string = Object.create(null);  // Should emit TS2322
    const b: string = String("hello");      // Should emit TS2322
    const c: string = JSON.parse('{}');     // Should emit TS2322 (any not assignable to string without assertion)
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    ctx.binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    checker.check_source_file(root);

    // Verify TS2322 errors were emitted for all three
    let diagnostics = &checker.ctx.diagnostics;
    let ts2322_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    assert!(ts2322_errors.len() >= 3, "Expected at least 3 TS2322 errors - proves globals are properly typed");
}

#[test]
fn test_any_poisoning_fix_undefined_global_emits_ts2304() {
    // Create a test context WITH lib.d.ts
    let ctx = TestContext::new();
    let mut checker = ctx.checker();

    // Parse code that references a truly undefined global
    let source = r#"
    const x = undefinedGlobalThatDoesNotExist;  // Should emit TS2304
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    ctx.binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    checker.check_source_file(root);

    // Verify TS2304 was emitted
    let diagnostics = &checker.ctx.diagnostics;
    let ts2304_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();

    assert!(!ts2304_errors.is_empty(), "Expected TS2304 error for truly undefined global");
}
