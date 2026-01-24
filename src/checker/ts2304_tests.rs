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
