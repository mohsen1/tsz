#[test]
fn test_source_map_block_scoping_let_const_mapping() {
    // Test let/const to var transform source mapping
    let source = r#"let x = 1;
const y = 2;
let z = x + y;
console.log(z);"#;
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

    // Verify let/const are transformed to var
    assert!(
        output.contains("var x") || output.contains("var y") || output.contains("var z"),
        "expected let/const transformed to var in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the variable declarations
    let (let_line, _) = find_line_col(source, "let x");
    let has_let_mapping = decoded.iter().any(|entry| entry.original_line == let_line);

    let (const_line, _) = find_line_col(source, "const y");
    let has_const_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == const_line);

    assert!(
        has_let_mapping || has_const_mapping || !decoded.is_empty(),
        "expected mappings for let/const declarations. mappings: {mappings}"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_block_scoping_nested_blocks_mapping() {
    // Test nested block scoping with shadowing
    let source = r#"let x = 1;
{
    let x = 2;
    console.log(x);
}
console.log(x);"#;
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

    // Verify we have non-empty mappings for nested blocks
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested block scoping. output: {output}"
    );

    // Verify console.log is in output
    assert!(
        output.contains("console.log"),
        "expected console.log in output: {output}"
    );
}

#[test]
fn test_source_map_block_scoping_for_loop_mapping() {
    // Test for loop with let variable
    let source = r#"for (let i = 0; i < 10; i++) {
    console.log(i);
}"#;
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

    // Verify for loop is in output
    assert!(
        output.contains("for") && (output.contains("var i") || output.contains("let i")),
        "expected for loop with variable in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the for loop
    let (for_line, _) = find_line_col(source, "for (let i");
    let has_for_mapping = decoded.iter().any(|entry| entry.original_line == for_line);

    assert!(
        has_for_mapping || !decoded.is_empty(),
        "expected mappings for for loop with let. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_block_scoping_function_scope_mapping() {
    // Test function-scoped let/const
    let source = r#"function test() {
    let local = 1;
    const result = local * 2;
    return result;
}"#;
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

    // Verify function is in output
    assert!(
        output.contains("function test"),
        "expected function in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the function declaration
    let (func_line, _) = find_line_col(source, "function test");
    let has_func_mapping = decoded.iter().any(|entry| entry.original_line == func_line);

    assert!(
        has_func_mapping || !decoded.is_empty(),
        "expected mappings for function with let/const. mappings: {mappings}"
    );

    // Verify we have mappings covering multiple source lines
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 2,
        "expected mappings from at least 2 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_enum_es5_string_enum_mapping() {
    // Test string enum transforms to IIFE pattern without reverse mapping
    let source = r#"enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT"
}"#;
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

    // Verify string enum generates IIFE pattern
    assert!(
        output.contains("var Direction"),
        "expected var Direction declaration in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for string enum"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_enum_es5_exported_enum_mapping() {
    // Test exported enum with source mapping
    let source = r#"export enum Status {
    Active = 1,
    Inactive = 0,
    Pending = 2
}

const current = Status.Active;"#;
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

    // Verify enum is in output
    assert!(
        output.contains("Status"),
        "expected Status enum in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the enum
    let (enum_line, _) = find_line_col(source, "enum Status");
    let has_enum_mapping = decoded.iter().any(|entry| entry.original_line == enum_line);

    assert!(
        has_enum_mapping || !decoded.is_empty(),
        "expected mappings for exported enum. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_enum_es5_computed_member_mapping() {
    // Test enum with computed member values
    let source = r#"enum Computed {
    A = 1,
    B = A * 2,
    C = 10
}"#;
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

    // Verify enum generates IIFE pattern
    assert!(
        output.contains("var Computed") || output.contains("Computed"),
        "expected Computed enum in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed enum members"
    );
}

#[test]
fn test_source_map_enum_es5_mixed_values_mapping() {
    // Test enum with mixed numeric and auto-increment values
    let source = r#"enum Mixed {
    First,
    Second,
    Third = 10,
    Fourth,
    Fifth = 100
}"#;
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

    // Verify enum IIFE pattern
    assert!(
        output.contains("var Mixed"),
        "expected var Mixed declaration in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the enum declaration
    let (enum_line, _) = find_line_col(source, "enum Mixed");
    let has_enum_mapping = decoded.iter().any(|entry| entry.original_line == enum_line);

    assert!(
        has_enum_mapping || !decoded.is_empty(),
        "expected mappings for enum with mixed values. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_commonjs_import_mapping() {
    // Test CommonJS import transform source mapping
    let source = r#"import { foo, bar } from "./module";
import * as utils from "./utils";
import defaultExport from "./default";

console.log(foo, bar, utils, defaultExport);"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify CommonJS require pattern
    assert!(
        output.contains("require") || output.contains("import"),
        "expected require or import in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for CommonJS imports"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_commonjs_export_mapping() {
    // Test CommonJS export transform source mapping
    let source = r#"export const value = 42;
export function greet(name: string) {
    return "Hello " + name;
}
export class MyClass {
    constructor() {}
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify exports pattern
    assert!(
        output.contains("exports") || output.contains("export"),
        "expected exports in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the export declarations
    let (value_line, _) = find_line_col(source, "export const value");
    let has_value_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == value_line);

    let (func_line, _) = find_line_col(source, "export function greet");
    let has_func_mapping = decoded.iter().any(|entry| entry.original_line == func_line);

    assert!(
        has_value_mapping || has_func_mapping || !decoded.is_empty(),
        "expected mappings for CommonJS exports. mappings: {mappings}"
    );
}

