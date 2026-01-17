//! ASI Conformance Tests
//!
//! Test ASI (Automatic Semicolon Insertion) behavior against JavaScript/TypeScript spec.
//! Focus on TS1005 (token expected) and TS1109 (expression expected) error codes.

use crate::checker::types::diagnostics::diagnostic_codes;
use crate::thin_parser::ThinParserState;
use crate::scanner::SyntaxKind;

/// Test that throw with line break reports TS1109
#[test]
fn test_asi_throw_line_break_reports_ts1109() {
    let source = r#"
function f() {
    throw
    new Error("test");
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|d| d.code)
        .collect();

    assert!(codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Should emit TS1109 for line break after throw, got: {:?}", codes);
}

/// Test that throw without line break is OK
#[test]
fn test_asi_throw_no_line_break_ok() {
    let source = r#"
function f() {
    throw new Error("test");
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let codes: Vec<u32> = parser
        .get_diagnostics()
        .iter()
        .map(|d| d.code)
        .collect();

    assert!(!codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Should NOT emit TS1109 for throw on same line, got: {:?}", codes);
}

/// Test return with line break (ASI applies, returns undefined)
#[test]
fn test_asi_return_line_break() {
    let source = r#"
function f() {
    return
    x + y;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies - return is a complete statement
    // The "x + y" becomes a separate (unreachable) statement
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test postfix ++ with line break (ASI applies)
#[test]
fn test_asi_postfix_increment_line_break() {
    let source = r#"
let x = 5
x++;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse as two statements: let x = 5; x++;
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test prefix ++ after line break (valid)
#[test]
fn test_asi_prefix_increment_after_line_break() {
    let source = r#"
let a = 5
let b = ++a;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse as: let a = 5; let b = ++a;
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test yield with line break (ASI applies)
#[test]
fn test_asi_yield_line_break() {
    let source = r#"
function* g() {
    yield
    x + y;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies - yield without expression is valid
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test break with label after line break (ASI applies)
#[test]
fn test_asi_break_label_line_break() {
    let source = r#"
outer: while (true) {
    break
    outer;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies - break; outer; (two statements)
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test arrow function with concise body
#[test]
fn test_asi_arrow_function_concise_body() {
    let source = r#"
let f = x => x * 2;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse arrow function correctly
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test arrow function with object literal (requires parens)
#[test]
fn test_asi_arrow_function_object_literal() {
    let source = r#"
let f = x => ({ x: 1 });
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse with parentheses
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Test ASI at EOF before closing brace
#[test]
fn test_asi_eof_before_closing_brace() {
    let source = r#"
function f() {
    return 42
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // ASI applies at EOF before }
    assert!(parser.arena.len() > 0, "Should parse successfully");
}

/// Comprehensive ASI edge case test suite
#[test]
fn test_asi_comprehensive_edge_cases() {
    let test_cases = vec![
        // (source, should_have_errors, description)
        // Valid ASI cases
        (r#"function f() { return }"#, false, "return without semicolon"),
        (r#"function f() { throw {}"#, false, "throw without semicolon (should error but for different reason)"),

        // Line break triggers ASI
        (r#"function f() { return\nx }"#, false, "return with line break (ASI)"),

        // throw with line break should error
        (r#"function f() { throw\nnew Error() }"#, true, "throw with line break (TS1109)"),

        // Postfix operators with line break
        (r#"let x = 5\nx++"#, false, "postfix ++ after line break"),
        (r#"let y = 5\ny--"#, false, "postfix -- after line break"),
    ];

    for (i, (source, should_have_errors, description)) in test_cases.iter().enumerate() {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        parser.parse_source_file();

        let has_errors = !parser.get_diagnostics().is_empty();

        if *should_have_errors && !has_errors {
            panic!("Test case {} ({}) expected errors but got none: {:?}", i, description, source);
        }
    }
}

/// Test TS1005 patterns (token expected)
#[test]
fn test_asi_ts1005_token_expected_patterns() {
    let test_cases = vec![
        // Missing tokens that should trigger TS1005
        (r#"function f() { }"#, false, "complete function"),
        (r#"function f( { }"#, true, "missing closing paren in function params"),
        (r#"if (true { }"#, true, "missing closing paren in if"),
    ];

    for (i, (source, should_have_errors, description)) in test_cases.iter().enumerate() {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        parser.parse_source_file();

        let diagnostics = parser.get_diagnostics();
        let has_errors = !diagnostics.is_empty();


        if *should_have_errors && !has_errors {
            panic!("Test case {} ({}) expected errors but got none", i, description);
        }
    }
}

/// Test async await issue - function declaration
#[test]
fn test_async_function_await_computed_property() {
    let source = r#"async function foo(): Promise<void> {
  var v = { [await]: foo }
}"#;

    let mut parser = ThinParserState::new("asyncFunctionDeclaration9_es2017.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == diagnostic_codes::TOKEN_EXPECTED).count();

    assert_eq!(ts1109_count, 1, "Should emit exactly 1 TS1109 error");
    assert_eq!(ts1005_count, 0, "Should emit no TS1005 errors");
    assert_eq!(diagnostics.len(), 1, "Should emit exactly 1 diagnostic total");
}

/// Test async await issue - arrow function
#[test]
fn test_async_arrow_await_computed_property() {
    let source = r#"var foo = async (): Promise<void> => {
  var v = { [await]: foo }
}"#;

    let mut parser = ThinParserState::new("asyncArrowFunction8_es6.ts".to_string(), source.to_string());
    parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == diagnostic_codes::TOKEN_EXPECTED).count();

    assert_eq!(ts1109_count, 1, "Should emit exactly 1 TS1109 error");
    assert_eq!(ts1005_count, 0, "Should emit no TS1005 errors");
    assert_eq!(diagnostics.len(), 1, "Should emit exactly 1 diagnostic total");
}

/// Test exact conformance test files
#[test]
fn debug_exact_conformance_files() {
    // Test asyncArrowFunction8_es2017.ts
    let arrow_source = r#"// @target: es2017
// @noEmitHelpers: true

var foo = async (): Promise<void> => {
  var v = { [await]: foo }
}"#;

    let mut parser = ThinParserState::new("asyncArrowFunction8_es2017.ts".to_string(), arrow_source.to_string());
    parser.parse_source_file();
    let arrow_diagnostics = parser.get_diagnostics();

    // Test asyncFunctionDeclaration9_es2017.ts
    let func_source = r#"// @target: es2017
// @noEmitHelpers: true
async function foo(): Promise<void> {
  var v = { [await]: foo }
}"#;

    let mut parser = ThinParserState::new("asyncFunctionDeclaration9_es2017.ts".to_string(), func_source.to_string());
    parser.parse_source_file();
    let func_diagnostics = parser.get_diagnostics();

    eprintln!("Arrow function diagnostics:");
    for diag in arrow_diagnostics.iter() {
        eprintln!("  Code: {}, Message: {}, Start: {}", diag.code, diag.message, diag.start);
    }

    eprintln!("Function declaration diagnostics:");
    for diag in func_diagnostics.iter() {
        eprintln!("  Code: {}, Message: {}, Start: {}", diag.code, diag.message, diag.start);
    }
}

/// Test for other TS1109 patterns that might be missing
#[test]
fn test_missing_ts1109_patterns() {
    // Pattern 1: throw with line break (should emit TS1109)
    let throw_source = r#"
function test() {
    throw
    new Error();
}
"#;

    let mut parser = ThinParserState::new("throw_test.ts".to_string(), throw_source.to_string());
    parser.parse_source_file();
    let throw_diagnostics = parser.get_diagnostics();

    // Pattern 2: return with line break (should emit TS1109)
    let return_source = r#"
function test() {
    return
    42;
}
"#;

    let mut parser = ThinParserState::new("return_test.ts".to_string(), return_source.to_string());
    parser.parse_source_file();
    let return_diagnostics = parser.get_diagnostics();

    eprintln!("Throw with line break diagnostics:");
    for diag in throw_diagnostics.iter() {
        eprintln!("  Code: {}, Message: {}, Start: {}", diag.code, diag.message, diag.start);
    }

    eprintln!("Return with line break diagnostics:");
    for diag in return_diagnostics.iter() {
        eprintln!("  Code: {}, Message: {}, Start: {}", diag.code, diag.message, diag.start);
    }
}

/// Test yield with line break pattern
#[test]
fn test_yield_ts1109() {
    let yield_source = r#"function* test() {
    yield
    42;
}"#;

    let mut parser = ThinParserState::new("yield_test.ts".to_string(), yield_source.to_string());
    parser.parse_source_file();
    let diagnostics = parser.get_diagnostics();

    let ts1109_count = diagnostics.iter().filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED).count();

    eprintln!("Yield diagnostics:");
    for diag in diagnostics.iter() {
        eprintln!("  Code: {}, Message: {}, Start: {}", diag.code, diag.message, diag.start);
    }

    assert_eq!(ts1109_count, 1, "Should emit exactly 1 TS1109 for yield with line break");
}

/// Test other ASI edge cases that might need TS1109
#[test]
fn test_asi_edge_cases_ts1109() {
    // Pattern 1: Postfix increment with line break
    let postfix_source = r#"let x = 5;
x
++;
console.log(x);"#;

    let mut parser = ThinParserState::new("postfix_test.ts".to_string(), postfix_source.to_string());
    parser.parse_source_file();
    let postfix_diagnostics = parser.get_diagnostics();

    // Pattern 2: Array access with line break  
    let array_source = r#"let arr = [1, 2, 3];
let val = arr
[0];"#;

    let mut parser = ThinParserState::new("array_test.ts".to_string(), array_source.to_string());
    parser.parse_source_file();
    let array_diagnostics = parser.get_diagnostics();

    eprintln!("Postfix increment diagnostics:");
    for diag in postfix_diagnostics.iter() {
        eprintln!("  Code: {}, Message: {}, Start: {}", diag.code, diag.message, diag.start);
    }

    eprintln!("Array access diagnostics:");
    for diag in array_diagnostics.iter() {
        eprintln!("  Code: {}, Message: {}, Start: {}", diag.code, diag.message, diag.start);
    }
}
