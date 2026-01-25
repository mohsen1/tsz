//! Tests for TS2304 emission ("Cannot find name")
//!
//! These tests verify that:
//! 1. TS2304 is emitted when referencing undefined names
//! 2. TS2304 is NOT emitted when lib.d.ts is loaded and provides the name
//! 3. The "Any poisoning" effect is eliminated

use crate::test_fixtures::TestContext;

#[test]
fn test_ts2304_emitted_for_undefined_name() {
    let mut ctx = TestContext::new_without_lib();
    let source = r#"const x = undefinedName;"#;
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder.bind_source_file(parser.get_arena(), root);
    let mut checker = ctx.checker();
    checker.check_source_file(root);
    let ts2304_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();
    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for undefinedName"
    );
}

#[test]
fn test_ts2304_not_emitted_for_lib_globals_with_lib() {
    let mut ctx = TestContext::new();
    let source = r#"console.log("hello");"#;
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder
        .bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);
    let mut checker = ctx.checker();
    checker.check_source_file(root);
    let ts2304_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for console with lib.d.ts"
    );
}

#[test]
fn test_ts2304_emitted_for_console_without_lib() {
    let mut ctx = TestContext::new_without_lib();
    let source = r#"console.log("hello");"#;
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder.bind_source_file(parser.get_arena(), root);
    let mut checker = ctx.checker();
    checker.check_source_file(root);
    let ts2304_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .collect();
    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 for console without lib.d.ts"
    );
}
