#[test]
fn test_source_map_exponentiation_in_expression() {
    // Test exponentiation in larger expression
    let source = r#"const a = 2;
const b = 3;
const result = a * (b ** 2) + 1;
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains variable name
    assert!(
        output.contains("result"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for exponentiation in expression"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_exponentiation_precedence() {
    // Test exponentiation with parentheses for precedence
    let source = r#"const a = (2 ** 3) ** 2;
const b = 2 ** (3 ** 2);
console.log(a, b);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains console
    assert!(
        output.contains("console"),
        "expected output to contain console. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for exponentiation precedence"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_exponentiation_in_function() {
    // Test exponentiation in function body
    let source = r#"function power(base: number, exp: number): number {
    return base ** exp;
}
const result = power(2, 10);
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains function name
    assert!(
        output.contains("power"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for exponentiation in function"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_exponentiation_with_method_call() {
    // Test exponentiation with method call
    let source = r#"function getExponent(): number {
    return 3;
}
const base = 2;
const result = base ** getExponent();
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains function name
    assert!(
        output.contains("getExponent"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for exponentiation with method call"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_exponentiation_combined() {
    // Test combined exponentiation patterns
    let source = r#"const base = 2;
let accumulator = 1;
accumulator **= base;
const squared = base ** 2;
const cubed = base ** 3;
const complex = (squared ** 2) + (cubed ** 2);
console.log(accumulator, squared, cubed, complex);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains variable names
    assert!(
        output.contains("accumulator"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined exponentiation"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// BigInt Literal Source Map Tests
// ============================================================================

#[test]
fn test_source_map_bigint_basic() {
    // Test basic BigInt literal
    let source = r#"const value = 123n;
console.log(value);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains variable name
    assert!(
        output.contains("value"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic BigInt"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_bigint_with_variables() {
    // Test BigInt with variable operations
    let source = r#"const a = 100n;
const b = 200n;
const sum = a + b;
console.log(sum);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains variable names
    assert!(
        output.contains("sum"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for BigInt with variables"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_bigint_arithmetic() {
    // Test BigInt arithmetic operations
    let source = r#"const a = 10n;
const b = 3n;
const add = a + b;
const sub = a - b;
const mul = a * b;
const div = a / b;
const mod = a % b;
console.log(add, sub, mul, div, mod);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains variable names
    assert!(
        output.contains("add"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for BigInt arithmetic"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_bigint_comparison() {
    // Test BigInt comparison operations
    let source = r#"const a = 10n;
const b = 20n;
const isLess = a < b;
const isEqual = a === 10n;
const isGreater = b > a;
console.log(isLess, isEqual, isGreater);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains variable names
    assert!(
        output.contains("isLess"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for BigInt comparison"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_bigint_constructor() {
    // Test BigInt constructor
    let source = r#"const fromNumber = BigInt(42);
const fromString = BigInt("12345678901234567890");
console.log(fromNumber, fromString);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify output contains variable names
    assert!(
        output.contains("fromNumber"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for BigInt constructor"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

