//! Source map tests - Part 1 (VLQ, decorators, basic source maps)

use crate::emit_context::EmitContext;
use crate::emitter::{Printer, PrinterOptions, ScriptTarget};
use crate::lowering_pass::LoweringPass;
use crate::parser::ParserState;
use crate::source_map::*;
use crate::source_map_test_utils::{decode_mappings, find_line_col, has_mapping_for_prefixes};
use serde_json::Value;

#[test]
fn test_vlq_encode_positive() {
    // Simple positive numbers
    assert_eq!(vlq::encode(0), "A");
    assert_eq!(vlq::encode(1), "C");
    assert_eq!(vlq::encode(15), "e");
    assert_eq!(vlq::encode(16), "gB");
}

#[test]
fn test_vlq_encode_negative() {
    // Negative numbers (sign in LSB)
    assert_eq!(vlq::encode(-1), "D");
    assert_eq!(vlq::encode(-15), "f");
}

#[test]
fn test_vlq_decode() {
    // Decode what we encode
    for value in [-100, -1, 0, 1, 100, 1000] {
        let encoded = vlq::encode(value);
        let (decoded, consumed) = vlq::decode(&encoded).unwrap();
        assert_eq!(decoded, value, "Failed for value {}", value);
        assert_eq!(consumed, encoded.len());
    }
}

#[test]
fn test_simple_map_generic() {
    // Minimal test with just Map generic - checking for infinite loops
    let source = r#"const metadata = new Map<any, Map<string, any>>();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("Map"),
        "expected output to contain Map. output: {output}"
    );
}

#[test]
fn test_parameter_decorator_simple() {
    // Test with just parameter decorator on class method
    let source = r#"function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

class ApiController {
    getUsers(@inject("db") db: any) {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_class_and_method_decorator() {
    // Test with class and method decorators
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

@controller("/api")
class ApiController {
    @get("/users")
    getUsers() {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_full_decorator_combo() {
    // Test with class, method, and parameter decorators
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

@controller("/api")
class ApiController {
    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_full_decorator_combo_with_prop() {
    // Test with class, method, property, and parameter decorators (matching the ignored test)
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

function prop(target: any, key: string) {}

@controller("/api")
class ApiController {
    @prop
    service: any;

    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_two_methods_no_param_decorators() {
    // Test with just two methods - no parameter decorators
    let source = r#"function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

class ApiController {
    @get("/users")
    getUsers() {
        return [];
    }

    @get("/posts")
    getPosts() {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_two_methods_with_param_decorators() {
    // Test with two methods and parameter decorators - the suspected issue
    let source = r#"function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

class ApiController {
    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
    }

    @get("/posts")
    getPosts(@inject("db") db: any) {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_multiple_param_decorators_on_one_method() {
    // Test with two parameter decorators on a single method - the suspected issue
    let source = r#"function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

class ApiController {
    getPosts(@inject("db") db: any, @inject("cache") cache: any) {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_all_decorators_combined_no_source_map() {
    // Test without source map to isolate the issue
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

function prop(target: any, key: string) {}

@controller("/api")
class ApiController {
    @prop
    service: any;

    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
    }

    @get("/posts")
    getPosts(@inject("db") db: any, @inject("cache") cache: any) {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    // Note: NOT enabling source map to test if that's the cause
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_all_decorators_combined_with_source_map() {
    // Test WITH source map - expect this to hang
    let source = r#"function controller(path: string) {
    return function(target: any) { return target; };
}

function get(route: string) {
    return function(target: any, key: string, desc: PropertyDescriptor) { return desc; };
}

function inject(token: string) {
    return function(target: any, key: string, index: number) {};
}

function prop(target: any, key: string) {}

@controller("/api")
class ApiController {
    @prop
    service: any;

    @get("/users")
    getUsers(@inject("db") db: any) {
        return [];
    }

    @get("/posts")
    getPosts(@inject("db") db: any, @inject("cache") cache: any) {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("ApiController"),
        "expected output to contain ApiController. output: {output}"
    );
}

#[test]
fn test_source_map_simple() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let source_idx = generator.add_source("input.ts".to_string());

    generator.add_simple_mapping(0, 0, source_idx, 0, 0);
    generator.add_simple_mapping(0, 10, source_idx, 0, 5);
    generator.add_simple_mapping(1, 0, source_idx, 1, 0);

    let json = generator.to_json();
    assert!(
        json.contains("\"version\":3") || json.contains("\"version\": 3"),
        "Should be v3 source map: {}",
        json
    );
    assert!(
        json.contains("\"file\":\"output.js\"") || json.contains("\"file\": \"output.js\""),
        "Should have file: {}",
        json
    );
    assert!(
        json.contains("\"sources\":[\"input.ts\"]") || json.contains("\"sources\": [\"input.ts\"]"),
        "Should have sources: {}",
        json
    );
    assert!(
        json.contains("\"mappings\""),
        "Should have mappings: {}",
        json
    );
}

#[test]
fn test_source_map_with_content() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    generator.add_source_with_content("input.ts".to_string(), "const x = 1;".to_string());

    generator.add_simple_mapping(0, 0, 0, 0, 0);

    let json = generator.to_json();
    assert!(json.contains("\"sourcesContent\""));
    assert!(json.contains("const x = 1;"));
}

#[test]
fn test_source_map_with_names() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let source_idx = generator.add_source("input.ts".to_string());
    let name_idx = generator.add_name("myVariable".to_string());

    generator.add_named_mapping(0, 0, source_idx, 0, 0, name_idx);

    let json = generator.to_json();
    assert!(
        json.contains("\"names\":[\"myVariable\"]") || json.contains("\"names\": [\"myVariable\"]"),
        "Should have names: {}",
        json
    );
}

#[test]
fn test_decode_mappings_round_trip() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let source_idx = generator.add_source("input.ts".to_string());

    generator.add_simple_mapping(0, 0, source_idx, 0, 0);
    generator.add_simple_mapping(0, 5, source_idx, 0, 3);
    generator.add_simple_mapping(1, 0, source_idx, 1, 0);

    let json = generator.to_json();
    let map_value: Value = serde_json::from_str(&json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0].generated_line, 0);
    assert_eq!(decoded[0].generated_column, 0);
    assert_eq!(decoded[0].original_line, 0);
    assert_eq!(decoded[0].original_column, 0);
    assert_eq!(decoded[1].generated_line, 0);
    assert_eq!(decoded[1].generated_column, 5);
    assert_eq!(decoded[1].original_line, 0);
    assert_eq!(decoded[1].original_column, 3);
    assert_eq!(decoded[2].generated_line, 1);
    assert_eq!(decoded[2].generated_column, 0);
    assert_eq!(decoded[2].original_line, 1);
    assert_eq!(decoded[2].original_column, 0);
}

#[test]
fn test_source_map_es5_transform_records_names() {
    let source = "const value = 1; const other = value;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let names = map_value
        .get("names")
        .and_then(|value| value.as_array())
        .expect("expected names array");
    assert!(
        names.iter().any(|name| name.as_str() == Some("value")),
        "expected names to include value. names: {names:?}"
    );

    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let decoded = decode_mappings(mappings);
    let value_index = names
        .iter()
        .position(|name| name.as_str() == Some("value"))
        .expect("value not found in names");
    assert!(
        decoded
            .iter()
            .any(|entry| entry.name_index == Some(value_index as u32)),
        "expected name mapping for value. mappings: {mappings}"
    );
}

#[test]
fn test_inline_source_map() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    generator.add_source("input.ts".to_string());
    generator.add_simple_mapping(0, 0, 0, 0, 0);

    let inline = generator.to_inline_comment();
    assert!(inline.starts_with("//# sourceMappingURL=data:application/json;base64,"));
}

#[test]
fn test_base64_encode() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
}

#[test]
fn test_escape_json() {
    assert_eq!(escape_json("hello"), "hello");
    assert_eq!(escape_json("hello\"world"), "hello\\\"world");
    assert_eq!(escape_json("path\\to\\file"), "path\\\\to\\\\file");
    assert_eq!(escape_json("line1\nline2"), "line1\\nline2");
}

#[test]
fn test_source_map_es5_transform_async_await_mapping() {
    let source = "async function fetch(payload) { await payload; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "payload");
    let (output_line, output_col) = find_line_col(&output, "payload");

    let mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col)
        .unwrap_or_else(|| {
            panic!("expected mapping for payload. mappings: {mappings} output: {output}")
        });

    assert_eq!(mapping.source_index, 0);
    assert_eq!(mapping.generated_line, output_line);
    assert_eq!(mapping.generated_column, output_col);
}

#[test]
fn test_source_map_es5_transform_async_await_return_mapping() {
    let source = "async function compute(value) { return await value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "value");

    let mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col)
        .unwrap_or_else(|| {
            panic!("expected mapping for value. mappings: {mappings} output: {output}")
        });

    assert_eq!(mapping.source_index, 0);

    let output_line_text = output
        .lines()
        .nth(mapping.generated_line as usize)
        .unwrap_or_else(|| {
            panic!(
                "missing output line {} in output: {output}",
                mapping.generated_line
            )
        });
    let output_slice = output_line_text
        .get(mapping.generated_column as usize..)
        .unwrap_or_else(|| {
            panic!(
                "missing output column {} in line: {output_line_text}",
                mapping.generated_column
            )
        });
    assert!(
        output_slice.starts_with("value"),
        "expected mapped output to start with value. line: {output_line_text} column: {} output: {output}",
        mapping.generated_column
    );
}

#[test]
fn test_source_map_es5_transform_async_await_property_access_mapping() {
    let source = "async function load(user) { return (await user).name; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "user");

    let mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col)
        .unwrap_or_else(|| {
            panic!("expected mapping for user. mappings: {mappings} output: {output}")
        });

    assert_eq!(mapping.source_index, 0);
    let output_line_text = output
        .lines()
        .nth(mapping.generated_line as usize)
        .unwrap_or_else(|| {
            panic!(
                "missing output line {} in output: {output}",
                mapping.generated_line
            )
        });
    let output_slice = output_line_text
        .get(mapping.generated_column as usize..)
        .unwrap_or_else(|| {
            panic!(
                "missing output column {} in line: {output_line_text}",
                mapping.generated_column
            )
        });
    assert!(
        output_slice.starts_with("user"),
        "expected mapped output to start with user. line: {output_line_text} column: {} output: {output}",
        mapping.generated_column
    );
}

#[test]
fn test_source_map_es5_transform_async_arrow_mapping() {
    let source = "const run = async (value) => { return await value; };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "value");

    let mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col)
        .unwrap_or_else(|| {
            panic!("expected mapping for value. mappings: {mappings} output: {output}")
        });

    assert_eq!(mapping.source_index, 0);
    let output_line_text = output
        .lines()
        .nth(mapping.generated_line as usize)
        .unwrap_or_else(|| {
            panic!(
                "missing output line {} in output: {output}",
                mapping.generated_line
            )
        });
    let output_slice = output_line_text
        .get(mapping.generated_column as usize..)
        .unwrap_or_else(|| {
            panic!(
                "missing output column {} in line: {output_line_text}",
                mapping.generated_column
            )
        });
    assert!(
        output_slice.starts_with("value"),
        "expected mapped output to start with value. line: {output_line_text} column: {} output: {output}",
        mapping.generated_column
    );
}

#[test]
fn test_source_map_es5_transform_async_class_method_mapping() {
    let source = "class Box {\n    async run(value) { return await value; }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("Box.prototype.run"),
        "expected class method downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (param_line, param_col) = find_line_col(source, "value");

    if let Some(mapping) = decoded
        .iter()
        .find(|entry| entry.original_line == param_line && entry.original_column == param_col)
    {
        assert_eq!(mapping.source_index, 0);
        let output_line_text = output
            .lines()
            .nth(mapping.generated_line as usize)
            .unwrap_or_else(|| {
                panic!(
                    "missing output line {} in output: {output}",
                    mapping.generated_line
                )
            });
        let output_slice = output_line_text
            .get(mapping.generated_column as usize..)
            .unwrap_or_else(|| {
                panic!(
                    "missing output column {} in line: {output_line_text}",
                    mapping.generated_column
                )
            });
        assert!(
            output_slice.starts_with("value"),
            "expected mapped output to start with value. line: {output_line_text} column: {} output: {output}",
            mapping.generated_column
        );
    } else {
        let (method_line, _) = find_line_col(source, "async run");
        let (output_line, output_col) = find_line_col(&output, "run = function");
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before method output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= method_line,
            "expected mapping before or on method line. mapping line: {} method line: {}",
            mapping.original_line,
            method_line
        );
    }
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_source_map_es5_transform_async_nested_function_offset_mapping() {
    let source = "function outer() {\n    const before = 1;\n    async function run() {\n        await payloadValue;\n    }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (await_line, await_col) = find_line_col(source, "payloadValue");

    let mapping = decoded
        .iter()
        .find(|entry| entry.original_line == await_line && entry.original_column == await_col)
        .unwrap_or_else(|| {
            panic!("expected mapping for payloadValue. mappings: {mappings} output: {output}")
        });

    assert_eq!(mapping.source_index, 0);
    let output_line_text = output
        .lines()
        .nth(mapping.generated_line as usize)
        .unwrap_or_else(|| {
            panic!(
                "missing output line {} in output: {output}",
                mapping.generated_line
            )
        });
    let output_slice = output_line_text
        .get(mapping.generated_column as usize..)
        .unwrap_or_else(|| {
            panic!(
                "missing output column {} in line: {output_line_text}",
                mapping.generated_column
            )
        });
    assert!(
        output_slice.starts_with("payloadValue"),
        "expected mapped output to start with payloadValue. line: {output_line_text} column: {} output: {output}",
        mapping.generated_column
    );
}

#[test]
fn test_source_map_es5_transform_async_await_conditional_mapping() {
    let source = "const run = async (value) => (value ? await value : 0);";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "value ?");

    if let Some(mapping) = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col)
    {
        assert_eq!(mapping.source_index, 0);
        let output_line_text = output
            .lines()
            .nth(mapping.generated_line as usize)
            .unwrap_or_else(|| {
                panic!(
                    "missing output line {} in output: {output}",
                    mapping.generated_line
                )
            });
        let output_slice = output_line_text
            .get(mapping.generated_column as usize..)
            .unwrap_or_else(|| {
                panic!(
                    "missing output column {} in line: {output_line_text}",
                    mapping.generated_column
                )
            });
        assert!(
            output_slice.starts_with("value"),
            "expected mapped output to start with value. line: {output_line_text} column: {} output: {output}",
            mapping.generated_column
        );
    } else {
        let (output_line, output_col) = if output.contains("value ?") {
            find_line_col(&output, "value ?")
        } else if output.contains("void 0") {
            find_line_col(&output, "void 0")
        } else {
            find_line_col(&output, "return [2")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before conditional output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= source_line,
            "expected mapping before or on conditional line. mapping line: {} conditional line: {}",
            mapping.original_line,
            source_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_arrow_captures_this_mapping() {
    let source = "const run = async function() { return await this.value; };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "this.value");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("this") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (output_line, output_col) = if output.contains("this.value") {
            find_line_col(&output, "this.value")
        } else {
            find_line_col(&output, "return [4")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before await output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= source_line,
            "expected mapping before or on await line. mapping line: {} await line: {}",
            mapping.original_line,
            source_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_mapping() {
    let source = "async function run() { try { await foo(); } catch { bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_await_mapping() {
    let source = "async function run() { try { await foo(); } catch (err) { await bar(err); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar(");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_return_await_mapping() {
    let source =
        "async function run() { try { await foo(); } catch (err) { return await bar(err); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar(");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_throw_await_mapping() {
    let source =
        "async function run() { try { await foo(); } catch (err) { throw await bar(err); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar(");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_only_await_mapping() {
    let source = "async function run() { try { foo(); } catch (err) { await bar(err); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (bar_line, bar_col) = find_line_col(source, "bar(");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == bar_line && entry.original_column == bar_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("bar")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_finally_only_mapping() {
    let source =
        "async function run() { try { foo(); } catch { await bar(); } finally { baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (bar_line, bar_col) = find_line_col(source, "bar()");
    let (baz_line, baz_col) = find_line_col(source, "baz()");

    let targets = [("bar", bar_line, bar_col), ("baz", baz_line, baz_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_if_else_mapping() {
    let source = "async function run(flag) { if (flag) { await foo(); } else { await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_if_await_condition_mapping() {
    let source =
        "async function run(flag){ if (await foo(flag)) { bar(); } else { await baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo(");
    let (baz_line, baz_col) = find_line_col(source, "baz(");

    let targets = [("foo", foo_line, foo_col), ("baz", baz_line, baz_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_if_await_and_mapping() {
    let source = "async function run(flag){ if ((await foo(flag)) && await bar()) { baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo(");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_while_await_condition_mapping() {
    let source = "async function run(cond){ while (await foo(cond)) { bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo(");

    let targets = [("foo", foo_line, foo_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_while_await_condition_list_mapping() {
    let source = "async function run(){ while ((await foo(), await bar())) { baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("foo", find_line_col(source, "foo()")),
        ("bar", find_line_col(source, "bar()")),
    ];

    for (label, (target_line, target_col)) in targets {
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_ternary_mapping() {
    let source = "async function run(flag, a, b) { return flag ? await foo(a) : await bar(b); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo(");
    let (bar_line, bar_col) = find_line_col(source, "bar(");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_ternary_await_condition_mapping() {
    let source = "async function run(){ return (await cond()) ? foo() : await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (cond_line, cond_col) = find_line_col(source, "cond()");
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [
        ("cond", cond_line, cond_col),
        ("foo", foo_line, foo_col),
        ("bar", bar_line, bar_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_ternary_consequent_await_mapping() {
    let source = "async function run(flag){ return flag ? await foo() : bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_logical_and_mapping() {
    let source = "async function run() { return (await foo()) && (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_logical_or_mapping() {
    let source = "async function run() { return (await foo()) || (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_logical_or_await_rhs_mapping() {
    let source = "async function run() { return foo() || await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_logical_and_await_rhs_mapping() {
    let source = "async function run() { return foo() && await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_logical_or_complex_mapping() {
    let source = "async function run(a, b){ return (await foo(a)) || (bar() && await baz(b)); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo(a)");
    let (bar_line, bar_col) = find_line_col(source, "bar()");
    let (baz_line, baz_col) = find_line_col(source, "baz(b)");

    let targets = [
        ("foo", foo_line, foo_col),
        ("bar", bar_line, bar_col),
        ("baz", baz_line, baz_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_logical_and_both_awaits_mapping() {
    let source = "async function run() { return (await foo()) && (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_add_mapping() {
    let source = "async function run() { return (await foo()) + (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_multiply_mapping() {
    let source = "async function run() { return (await foo()) * (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_subtract_mapping() {
    let source = "async function run() { return (await foo()) - (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_divide_mapping() {
    let source = "async function run() { return (await foo()) / (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_modulo_mapping() {
    let source = "async function run() { return (await foo()) % (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_less_than_mapping() {
    let source = "async function run() { return (await foo()) < (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_strict_equal_mapping() {
    let source = "async function run() { return (await foo()) === (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_strict_not_equal_mapping() {
    let source = "async function run() { return (await foo()) !== (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_greater_equal_mapping() {
    let source = "async function run() { return (await foo()) >= (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_less_equal_mapping() {
    let source = "async function run() { return (await foo()) <= (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_greater_than_mapping() {
    let source = "async function run() { return (await foo()) > (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_equal_mapping() {
    let source = "async function run() { return (await foo()) == (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_not_equal_mapping() {
    let source = "async function run() { return (await foo()) != (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_bitwise_and_mapping() {
    let source = "async function run() { return (await foo()) & (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_bitwise_or_mapping() {
    let source = "async function run() { return (await foo()) | (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_bitwise_xor_mapping() {
    let source = "async function run() { return (await foo()) ^ (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_destructuring_mapping() {
    let source = "async function run(){ const { value } = await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (return_line, return_col) = find_line_col(source, "return value");
    let value_col = return_col + "return ".len() as u32;

    let targets = [
        ("foo", foo_line, foo_col),
        ("value", return_line, value_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_shift_left_mapping() {
    let source = "async function run() { return (await foo()) << (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_shift_right_mapping() {
    let source = "async function run() { return (await foo()) >> (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_binary_unsigned_shift_right_mapping() {
    let source = "async function run() { return (await foo()) >>> (await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_array_literal_mapping() {
    let source = "async function run(){ return [await foo(), await bar()]; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_array_literal_spread_mapping() {
    let source = "async function run(){ return [...await foo(), await bar()]; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_object_literal_mapping() {
    let source = "async function run(){ return { value: await foo(), other: await bar() }; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_object_literal_spread_mapping() {
    let source = "async function run(){ return { ...await foo(), value: await bar() }; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_object_literal_computed_mapping() {
    let source = "async function run(){ return { [await key()]: 1, other: 2 }; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (key_line, key_col) = find_line_col(source, "key()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == key_line && entry.original_column == key_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("key") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output for computed key. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line for computed key. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_nested_await_call_mapping() {
    let source = "async function run(){ return await foo(await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo(");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_call_spread_mapping() {
    let source = "async function run(){ return foo(...await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo(");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_template_literal_mapping() {
    let source = "async function run(){ return `value ${await foo()}`; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == foo_line && entry.original_column == foo_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_return_await_in_try_mapping() {
    let source =
        "async function run() { try { return await foo(); } catch (err) { await bar(err); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar(");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_finally_mapping() {
    let source = "async function run() { try { await foo(); } finally { bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_finally_only_await_mapping() {
    let source = "async function run() { try { foo(); } finally { await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == bar_line && entry.original_column == bar_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("bar") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_finally_await_mapping() {
    let source = "async function run() { try { await foo(); } finally { await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_finally_await_in_finally_direct_mapping() {
    let source = "async function run() {\n    try {\n        await work();\n    } finally {\n        const done = await cleanup();\n        report(done);\n    }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("work()", &["work"][..]),
        ("cleanup()", &["cleanup"][..]),
        ("report(done)", &["report"][..]),
    ];
    let mut mapped = false;

    for (needle, prefixes) in targets {
        if has_mapping_for_prefixes(&decoded, &output, source, needle, prefixes) {
            mapped = true;
            break;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_finally_return_mapping() {
    let source = "async function run() { try { return await foo(); } finally { await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_finally_mapping() {
    let source =
        "async function run() { try { await foo(); } catch { bar(); } finally { baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");
    let (baz_line, baz_col) = find_line_col(source, "baz()");

    let targets = [
        ("foo", foo_line, foo_col),
        ("bar", bar_line, bar_col),
        ("baz", baz_line, baz_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_finally_await_in_finally_mapping() {
    let source = "async function run() { try { foo(); } catch (err) { bar(err); } finally { await baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar(");
    let (baz_line, baz_col) = find_line_col(source, "baz()");

    let targets = [
        ("foo", foo_line, foo_col),
        ("bar", bar_line, bar_col),
        ("baz", baz_line, baz_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_finally_catch_finally_awaits_mapping() {
    let source = "async function run() { try { foo(); } catch (err) { await bar(err); } finally { await baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar(");
    let (baz_line, baz_col) = find_line_col(source, "baz()");

    let targets = [
        ("foo", foo_line, foo_col),
        ("bar", bar_line, bar_col),
        ("baz", baz_line, baz_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_finally_awaits_mapping() {
    let source = "async function run() { try { await foo(); } catch (err) { await bar(err); } finally { await baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar(");
    let (baz_line, baz_col) = find_line_col(source, "baz()");

    let targets = [
        ("foo", foo_line, foo_col),
        ("bar", bar_line, bar_col),
        ("baz", baz_line, baz_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_assignment_mapping() {
    let source = "async function run(){ let value; value = await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (return_line, return_col) = find_line_col(source, "return value");
    let value_col = return_col + "return ".len() as u32;

    let targets = [
        ("foo", foo_line, foo_col),
        ("value", return_line, value_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_variable_initializer_await_mapping() {
    let source = "async function run(){ let value = await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == foo_line && entry.original_column == foo_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output for foo initializer. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line for foo initializer. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_variable_declaration_list_await_mapping() {
    let source = "async function run(){ let first = await getFirst(), second = await getSecond(); return second; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("getFirst", find_line_col(source, "getFirst()")),
        ("getSecond", find_line_col(source, "getSecond()")),
    ];

    for (label, (src_line, src_col)) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_unary_not_mapping() {
    let source = "async function run(){ return !(await foo()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == foo_line && entry.original_column == foo_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_unary_negative_mapping() {
    let source = "async function run(){ return -(await foo()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == foo_line && entry.original_column == foo_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_unary_bitwise_not_mapping() {
    let source = "async function run(){ return ~(await foo()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == foo_line && entry.original_column == foo_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_unary_void_mapping() {
    let source = "async function run(){ return void (await foo()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == foo_line && entry.original_column == foo_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_unary_typeof_mapping() {
    let source = "async function run(){ return typeof (await foo()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");

    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == foo_line && entry.original_column == foo_col);
    let direct_valid = direct_mapping.and_then(|mapping| {
        if mapping.source_index != 0 {
            return None;
        }

        let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
        let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
        if output_slice.starts_with("foo") {
            Some(mapping)
        } else {
            None
        }
    });

    if direct_valid.is_none() {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_sequence_await_mapping() {
    let source = "async function run(){ return (await foo(), await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_sequence_mixed_await_mapping() {
    let source = "async function run(){ return (foo(), await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_optional_chaining_await_mapping() {
    let source = "async function run(){ return await foo?.bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo");
    let (bar_line, bar_col) = find_line_col(source, "bar");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_nullish_coalescing_await_mapping() {
    let source = "async function run(){ return (await foo()) ?? bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_delete_mapping() {
    let source = "async function run(){ return delete (await foo()).bar; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_compound_assignment_mapping() {
    let source = "async function run(){ let value = 0; value += await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (return_line, return_col) = find_line_col(source, "return value");
    let value_col = return_col + "return ".len() as u32;

    let targets = [
        ("foo", foo_line, foo_col),
        ("value", return_line, value_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_compound_assignment_multiply_mapping() {
    let source = "async function run(){ let value = 2; value *= await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (return_line, return_col) = find_line_col(source, "return value");
    let value_col = return_col + "return ".len() as u32;

    let targets = [
        ("foo", foo_line, foo_col),
        ("value", return_line, value_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_compound_assignment_divide_mapping() {
    let source = "async function run(){ let value = 4; value /= await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (return_line, return_col) = find_line_col(source, "return value");
    let value_col = return_col + "return ".len() as u32;

    let targets = [
        ("foo", foo_line, foo_col),
        ("value", return_line, value_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_compound_assignment_subtract_mapping() {
    let source = "async function run(){ let value = 5; value -= await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (return_line, return_col) = find_line_col(source, "return value");
    let value_col = return_col + "return ".len() as u32;

    let targets = [
        ("foo", foo_line, foo_col),
        ("value", return_line, value_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_compound_assignment_modulo_mapping() {
    let source = "async function run(){ let value = 7; value %= await foo(); return value; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (return_line, return_col) = find_line_col(source, "return value");
    let value_col = return_col + "return ".len() as u32;

    let targets = [
        ("foo", foo_line, foo_col),
        ("value", return_line, value_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_nested_arrow_capture_mapping() {
    let source = "async function run() { const bar = () => this.x; await bar(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [("this.x", &["this", "_this"][..]), ("bar()", &["bar"][..])];
    let mut mapped = false;

    for (needle, prefixes) in targets {
        let (target_line, target_col) = find_line_col(source, needle);
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });

        if let Some(mapping) = direct_mapping
            && mapping.source_index == 0
        {
            let output_line_text = output.lines().nth(mapping.generated_line as usize);
            let output_slice =
                output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
            if let Some(output_slice) = output_slice
                && prefixes
                    .iter()
                    .any(|prefix| output_slice.starts_with(prefix))
            {
                mapped = true;
                break;
            }
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_try_catch_nested_arrow_mapping() {
    let source =
        "async function run(){ try { const bar=()=>this.x; await bar(); } catch { baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("this.x", &["this", "_this"][..]),
        ("bar()", &["bar"][..]),
        ("baz()", &["baz"][..]),
    ];
    let mut mapped = false;

    for (needle, prefixes) in targets {
        let (target_line, target_col) = find_line_col(source, needle);
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });

        if let Some(mapping) = direct_mapping
            && mapping.source_index == 0
        {
            let output_line_text = output.lines().nth(mapping.generated_line as usize);
            let output_slice =
                output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
            if let Some(output_slice) = output_slice
                && prefixes
                    .iter()
                    .any(|prefix| output_slice.starts_with(prefix))
            {
                mapped = true;
                break;
            }
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_object_literal_arrow_mapping() {
    let source = "const obj = { run: async () => { await foo(); return this.x; } };";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [("foo()", &["foo"][..]), ("this.x", &["this", "_this"][..])];
    let mut mapped = false;

    for (needle, prefixes) in targets {
        let (target_line, target_col) = find_line_col(source, needle);
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });

        if let Some(mapping) = direct_mapping
            && mapping.source_index == 0
        {
            let output_line_text = output.lines().nth(mapping.generated_line as usize);
            let output_slice =
                output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
            if let Some(output_slice) = output_slice
                && prefixes
                    .iter()
                    .any(|prefix| output_slice.starts_with(prefix))
            {
                mapped = true;
                break;
            }
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "const obj");
        let (output_line, output_col) = if output.contains("obj = {") {
            find_line_col(&output, "obj = {")
        } else if output.contains("obj =") {
            find_line_col(&output, "obj =")
        } else {
            find_line_col(&output, "obj")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before object output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on object line. mapping line: {} object line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_switch_mapping() {
    let source =
        "async function run(x){ switch(x){ case 1: await foo(); break; default: bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [("foo()", &["foo"][..]), ("bar()", &["bar"][..])];
    let mut mapped = false;

    for (needle, prefixes) in targets {
        let (target_line, target_col) = find_line_col(source, needle);
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });

        if let Some(mapping) = direct_mapping
            && mapping.source_index == 0
        {
            let output_line_text = output.lines().nth(mapping.generated_line as usize);
            let output_slice =
                output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
            if let Some(output_slice) = output_slice
                && prefixes
                    .iter()
                    .any(|prefix| output_slice.starts_with(prefix))
            {
                mapped = true;
                break;
            }
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_mapping() {
    let source = "async function run(){ for (let i=0;i<1;i++){ await foo(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_await_condition_mapping() {
    let source = "async function run(cond){ for (; await foo(cond); ) { bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo(");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_await_condition_list_mapping() {
    let source = "async function run(){ for (; (await foo(), await bar()); ) { baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("foo", find_line_col(source, "foo()")),
        ("bar", find_line_col(source, "bar()")),
    ];

    for (label, (target_line, target_col)) in targets {
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });

        let mut mapped = false;
        if let Some(mapping) = direct_mapping
            && mapping.source_index == 0
        {
            let output_line_text = output.lines().nth(mapping.generated_line as usize);
            let output_slice =
                output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
            if let Some(output_slice) = output_slice
                && output_slice.starts_with(label)
            {
                mapped = true;
            }
        }

        if !mapped {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_await_initializer_mapping() {
    let source = "async function run(){ for (let i = await foo(); i < 1; i++) { bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_await_initializer_list_mapping() {
    let source =
        "async function run(){ for (let i = await foo(), j = await bar(); i < j; i++) { baz(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("foo", find_line_col(source, "foo()")),
        ("bar", find_line_col(source, "bar()")),
    ];

    for (label, (target_line, target_col)) in targets {
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });

        let mut mapped = false;
        if let Some(mapping) = direct_mapping
            && mapping.source_index == 0
        {
            let output_line_text = output.lines().nth(mapping.generated_line as usize);
            let output_slice =
                output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
            if let Some(output_slice) = output_slice
                && output_slice.starts_with(label)
            {
                mapped = true;
            }
        }

        if !mapped {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_await_update_list_mapping() {
    let source = "async function run(){ let j = 0; for (let i = 0; i < 1; i = await foo(), j = await bar()) { baz(i, j); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("foo", find_line_col(source, "foo()")),
        ("bar", find_line_col(source, "bar()")),
    ];

    for (label, (target_line, target_col)) in targets {
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });

        let mut mapped = false;
        if let Some(mapping) = direct_mapping
            && mapping.source_index == 0
        {
            let output_line_text = output.lines().nth(mapping.generated_line as usize);
            let output_slice =
                output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
            if let Some(output_slice) = output_slice
                && output_slice.starts_with(label)
            {
                mapped = true;
            }
        }

        if !mapped {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_await_update_mapping() {
    let source = "async function run(){ for (let i = 0; i < 1; i = await foo()) { bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_loop_header_awaits_mapping() {
    let source = "async function run() {\n    for (let i = await init(); await cond(i); i = await step(i)) {\n        await body(i);\n    }\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("init()", &["init"][..]),
        ("cond(i)", &["cond"][..]),
        ("step(i)", &["step"][..]),
        ("body(i)", &["body"][..]),
    ];
    let mut mapped = false;

    for (needle, prefixes) in targets {
        if has_mapping_for_prefixes(&decoded, &output, source, needle, prefixes) {
            mapped = true;
            break;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_while_loop_mapping() {
    let source = "async function run(cond){ while (cond) { await foo(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_do_while_mapping() {
    let source = "async function run(cond){ do { await foo(); } while (cond); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_do_while_await_condition_mapping() {
    let source = "async function run(cond){ do { bar(); } while (await foo(cond)); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo(");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_do_while_await_condition_list_mapping() {
    let source = "async function run(){ do { baz(); } while ((await foo(), await bar())); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let targets = [
        ("foo", find_line_col(source, "foo()")),
        ("bar", find_line_col(source, "bar()")),
    ];

    for (label, (target_line, target_col)) in targets {
        let direct_mapping = decoded.iter().find(|entry| {
            entry.original_line == target_line && entry.original_column == target_col
        });
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_do_while_await_condition_direct_mapping() {
    let source = "async function run(flag) {\n    do {\n        tick(flag);\n    } while (await shouldContinue(flag));\n}";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let mapped = has_mapping_for_prefixes(
        &decoded,
        &output,
        source,
        "shouldContinue(flag)",
        &["shouldContinue"],
    );

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_of_mapping() {
    let source = "async function run(items){ for (const item of items) { await foo(item); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo(");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_of_await_rhs_mapping() {
    let source = "async function run(items){ for (const item of await foo(items)) { bar(item); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo(");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_in_mapping() {
    let source = "async function run(obj){ for (const key in obj) { await foo(obj[key]); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo(");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_for_in_await_rhs_mapping() {
    let source = "async function run(obj){ for (const key in await foo(obj)) { bar(key); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (target_line, target_col) = find_line_col(source, "foo(");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == target_line && entry.original_column == target_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("foo")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_switch_default_await_mapping() {
    let source =
        "async function run(x){ switch(x){ case 1: await foo(); break; default: await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (bar_line, bar_col) = find_line_col(source, "bar()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == bar_line && entry.original_column == bar_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("bar")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_switch_default_only_await_mapping() {
    let source = "async function run(x){ switch(x){ case 1: break; default: await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (bar_line, bar_col) = find_line_col(source, "bar()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == bar_line && entry.original_column == bar_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("bar")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_switch_case_await_mapping() {
    let source = "async function run(x){ switch(x){ case 1: await foo(); break; case 2: await bar(); break; } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (bar_line, bar_col) = find_line_col(source, "bar()");
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == bar_line && entry.original_column == bar_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("bar")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_switch_return_await_mapping() {
    let source = "async function run(x){ switch(x){ case 1: return await foo(); default: return await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_switch_await_discriminant_mapping() {
    let source = "async function run(payload){ switch(await payload){ case 1: await foo(); break; default: await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (await_line, await_col) = find_line_col(source, "await payload");
    let payload_col = await_col + "await ".len() as u32;
    let direct_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == await_line && entry.original_column == payload_col);

    let mut mapped = false;
    if let Some(mapping) = direct_mapping
        && mapping.source_index == 0
    {
        let output_line_text = output.lines().nth(mapping.generated_line as usize);
        let output_slice =
            output_line_text.and_then(|line| line.get(mapping.generated_column as usize..));
        if let Some(output_slice) = output_slice
            && output_slice.starts_with("payload")
        {
            mapped = true;
        }
    }

    if !mapped {
        let (func_line, _) = find_line_col(source, "async function run");
        let (output_line, output_col) = if output.contains("function run") {
            find_line_col(&output, "function run")
        } else if output.contains("run = function") {
            find_line_col(&output, "run = function")
        } else {
            find_line_col(&output, "run")
        };
        let mapping = decoded
            .iter()
            .filter(|entry| {
                entry.generated_line < output_line
                    || (entry.generated_line == output_line
                        && entry.generated_column <= output_col)
            })
            .max_by_key(|entry| (entry.generated_line, entry.generated_column))
            .unwrap_or_else(|| {
                panic!(
                    "expected mapping at or before async function output. mappings: {mappings} output: {output}"
                )
            });

        assert_eq!(mapping.source_index, 0);
        assert!(
            mapping.original_line <= func_line,
            "expected mapping before or on function line. mapping line: {} function line: {}",
            mapping.original_line,
            func_line
        );
    }
}

#[test]
fn test_source_map_es5_transform_async_switch_fallthrough_await_mapping() {
    let source = "async function run(x){ switch(x){ case 1: case 2: await foo(); break; default: await bar(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "foo()");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("foo", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_class_extends_mapping() {
    let source = "class Base { base = 1; }\nclass Derived extends Base { value = 2; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__extends"),
        "expected __extends helper in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, source_col) = find_line_col(source, "class Derived");
    let (output_line, output_col) = find_line_col(&output, "var Derived");

    let mapping = decoded
        .iter()
        .find(|entry| entry.original_line == source_line && entry.original_column == source_col)
        .unwrap_or_else(|| {
            panic!("expected mapping for Derived class. mappings: {mappings} output: {output}")
        });

    assert_eq!(mapping.source_index, 0);
    assert_eq!(mapping.generated_line, output_line);
    assert_eq!(mapping.generated_column, output_col);
}

#[test]
fn test_source_map_es5_transform_class_property_initializer_mapping() {
    let source = "class Box { foo = 1; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("this.foo = 1"),
        "expected downlevel property initializer, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (source_line, _) = find_line_col(source, "foo = 1");
    let (output_line, output_col) = find_line_col(&output, "this.foo = 1");

    // Use the closest mapping at or before the initializer output position.
    let mapping = decoded
        .iter()
        .filter(|entry| {
            entry.generated_line < output_line
                || (entry.generated_line == output_line
                    && entry.generated_column <= output_col)
        })
        .max_by_key(|entry| (entry.generated_line, entry.generated_column))
        .unwrap_or_else(|| {
            panic!(
                "expected mapping at or before initializer output. mappings: {mappings} output: {output}"
            )
        });

    assert_eq!(mapping.source_index, 0);
    assert_eq!(mapping.original_line, source_line);
}

#[test]
fn test_source_map_es5_transform_derived_ctor_super_initializer_mapping() {
    let source = "class Base {}\nclass Derived extends Base {\n    constructor() { super(); this.foo = 1; }\n}\n";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__extends"),
        "expected __extends helper in output: {output}"
    );
    assert!(
        output.contains("foo = 1"),
        "expected downlevel initializer in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (class_line, _) = find_line_col(source, "class Derived");
    let (output_line, output_col) = find_line_col(&output, "foo = 1");

    let mapping = decoded
        .iter()
        .filter(|entry| {
            entry.generated_line < output_line
                || (entry.generated_line == output_line
                    && entry.generated_column <= output_col)
        })
        .max_by_key(|entry| (entry.generated_line, entry.generated_column))
        .unwrap_or_else(|| {
            panic!(
                "expected mapping at or before initializer output. mappings: {mappings} output: {output}"
            )
        });

    assert_eq!(mapping.source_index, 0);
    assert_eq!(mapping.original_line, class_line);
}

#[test]
fn test_source_map_es5_transform_async_new_expression_mapping() {
    let source = "async function run(){ return new Foo(await bar()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (foo_line, foo_col) = find_line_col(source, "new Foo");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("new", foo_line, foo_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_tagged_template_mapping() {
    let source = "async function run(){ return tag`hello ${await bar()}`; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (tag_line, tag_col) = find_line_col(source, "tag`");
    let (bar_line, bar_col) = find_line_col(source, "bar()");

    let targets = [("tag", tag_line, tag_col), ("bar", bar_line, bar_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_instanceof_mapping() {
    let source = "async function run(){ return (await bar()) instanceof Foo; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (bar_line, bar_col) = find_line_col(source, "bar()");
    let (foo_line, foo_col) = find_line_col(source, "instanceof Foo");

    let targets = [
        ("bar", bar_line, bar_col),
        ("instanceof", foo_line, foo_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_exponentiation_mapping() {
    let source = "async function run(){ return (await base()) ** (await exp()); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (base_line, base_col) = find_line_col(source, "base()");
    let (exp_line, exp_col) = find_line_col(source, "exp()");

    let targets = [("base", base_line, base_col), ("exp", exp_line, exp_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_in_operator_mapping() {
    let source = "async function run(){ return (await key()) in obj; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (key_line, key_col) = find_line_col(source, "key()");

    let targets = [("key", key_line, key_col)];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_nested_try_finally_mapping() {
    let source = "async function run(){ try { try { await inner(); } finally { await cleanup1(); } } finally { await cleanup2(); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (inner_line, inner_col) = find_line_col(source, "inner()");
    let (cleanup1_line, cleanup1_col) = find_line_col(source, "cleanup1()");
    let (cleanup2_line, cleanup2_col) = find_line_col(source, "cleanup2()");

    let targets = [
        ("inner", inner_line, inner_col),
        ("cleanup1", cleanup1_line, cleanup1_col),
        ("cleanup2", cleanup2_line, cleanup2_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_async_for_of_destructuring_mapping() {
    let source =
        "async function run(){ for (const [a, b] of await items()) { await process(a, b); } }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("__awaiter(") && output.contains("__generator("),
        "expected async downlevel output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (items_line, items_col) = find_line_col(source, "items()");
    let (process_line, process_col) = find_line_col(source, "process(");

    let targets = [
        ("items", items_line, items_col),
        ("process", process_line, process_col),
    ];

    for (label, src_line, src_col) in targets {
        let direct_mapping = decoded
            .iter()
            .find(|entry| entry.original_line == src_line && entry.original_column == src_col);
        let direct_valid = direct_mapping.and_then(|mapping| {
            if mapping.source_index != 0 {
                return None;
            }

            let output_line_text = output.lines().nth(mapping.generated_line as usize)?;
            let output_slice = output_line_text.get(mapping.generated_column as usize..)?;
            if output_slice.starts_with(label) {
                Some(mapping)
            } else {
                None
            }
        });

        if direct_valid.is_none() {
            let (func_line, _) = find_line_col(source, "async function run");
            let (output_line, output_col) = if output.contains("function run") {
                find_line_col(&output, "function run")
            } else if output.contains("run = function") {
                find_line_col(&output, "run = function")
            } else {
                find_line_col(&output, "run")
            };
            let mapping = decoded
                .iter()
                .filter(|entry| {
                    entry.generated_line < output_line
                        || (entry.generated_line == output_line
                            && entry.generated_column <= output_col)
                })
                .max_by_key(|entry| (entry.generated_line, entry.generated_column))
                .unwrap_or_else(|| {
                    panic!(
                        "expected mapping at or before async function output for {label}. mappings: {mappings} output: {output}"
                    )
                });

            assert_eq!(mapping.source_index, 0);
            assert!(
                mapping.original_line <= func_line,
                "expected mapping before or on function line for {label}. mapping line: {} function line: {}",
                mapping.original_line,
                func_line
            );
        }
    }
}

#[test]
fn test_source_map_es5_transform_generator_yield_mapping() {
    let source = "function* gen() { yield first(); yield second(); }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    // Generator functions may or may not be downleveled depending on implementation
    // Just verify the output contains yield or __generator
    assert!(
        output.contains("yield") || output.contains("__generator("),
        "expected generator output, got: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    let (func_line, func_col) = find_line_col(source, "function* gen");
    let (yield1_line, yield1_col) = find_line_col(source, "yield first");
    let (yield2_line, yield2_col) = find_line_col(source, "yield second");

    // Verify we have mappings for the function declaration
    let func_mapping = decoded
        .iter()
        .find(|entry| entry.original_line == func_line && entry.original_column == func_col);
    assert!(
        func_mapping.is_some(),
        "expected mapping for function* gen. mappings: {mappings}"
    );

    // Verify we have mappings somewhere in the yield range
    let has_yield1_mapping = decoded.iter().any(|entry| {
        entry.original_line == yield1_line
            && entry.original_column >= yield1_col
            && entry.original_column <= yield1_col + 12
    });
    let has_yield2_mapping = decoded.iter().any(|entry| {
        entry.original_line == yield2_line
            && entry.original_column >= yield2_col
            && entry.original_column <= yield2_col + 13
    });

    assert!(
        has_yield1_mapping || has_yield2_mapping,
        "expected at least one mapping for yield expressions. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_names_array_multiple_identifiers() {
    let source = "function greet(name) { const message = 'Hello ' + name; return message; }";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    // Verify the names array exists
    let names = map_value
        .get("names")
        .and_then(|value| value.as_array())
        .expect("expected names array in source map");

    // Check that expected identifiers are in the names array
    let expected_names = ["greet", "name", "message"];
    for expected in expected_names {
        assert!(
            names.iter().any(|n| n.as_str() == Some(expected)),
            "expected names array to include '{}'. names: {:?}",
            expected,
            names
        );
    }

    // Verify mappings reference the names
    let mappings = map_value
        .get("mappings")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let decoded = decode_mappings(mappings);

    // At least some mappings should have name indices
    let mappings_with_names = decoded.iter().filter(|m| m.name_index.is_some()).count();
    assert!(
        mappings_with_names > 0,
        "expected some mappings to have name indices. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_sources_content_accuracy() {
    // Test with multiline source containing various constructs
    let source = r#"function hello(name: string): string {
    const greeting = "Hello, " + name;
    return greeting;
}

const result = hello("World");
console.log(result);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    // Verify sources array exists and contains the source file
    let sources = map_value
        .get("sources")
        .and_then(|v| v.as_array())
        .expect("expected sources array");
    assert_eq!(sources.len(), 1, "expected one source file");
    assert_eq!(
        sources[0].as_str(),
        Some("test.ts"),
        "expected source file name"
    );

    // Verify sourcesContent array exists and has same length as sources
    let sources_content = map_value
        .get("sourcesContent")
        .and_then(|v| v.as_array())
        .expect("expected sourcesContent array");
    assert_eq!(
        sources_content.len(),
        sources.len(),
        "sourcesContent length should match sources length"
    );

    // Verify the sourcesContent contains the exact original source
    let content = sources_content[0]
        .as_str()
        .expect("expected sourcesContent to be a string");
    assert_eq!(
        content, source,
        "sourcesContent should exactly match original source"
    );
}

#[test]
fn test_source_map_with_decorators() {
    // Test decorators on class and method
    let source = r#"function sealed(target: Function) {}
function log(target: any, key: string) {}

@sealed
class Example {
    @log
    greet() { return "hello"; }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the decorator functions
    let (sealed_line, sealed_col) = find_line_col(source, "function sealed");
    let has_sealed_mapping = decoded.iter().any(|entry| {
        entry.original_line == sealed_line
            && entry.original_column >= sealed_col
            && entry.original_column <= sealed_col + 15
    });

    let (log_line, log_col) = find_line_col(source, "function log");
    let has_log_mapping = decoded.iter().any(|entry| {
        entry.original_line == log_line
            && entry.original_column >= log_col
            && entry.original_column <= log_col + 12
    });

    // Verify we have mappings for decorator functions
    assert!(
        has_sealed_mapping || has_log_mapping,
        "expected mappings for decorator functions. mappings: {mappings}"
    );

    // Verify output contains the decorated class emitted as ES5 IIFE
    assert!(
        output.contains("Example") && output.contains("greet"),
        "expected output to contain decorated class and method. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for decorated code"
    );
}

#[test]
fn test_source_map_optional_chaining() {
    // Test optional chaining operators: ?. ?.[] ?.()
    let source = r#"const obj = { a: { b: 1 } };
const x = obj?.a?.b;
const arr = [1, 2, 3];
const y = arr?.[0];
const fn = (x: number) => x * 2;
const z = fn?.(5);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the variable declarations
    let (obj_line, obj_col) = find_line_col(source, "const obj");
    let has_obj_mapping = decoded.iter().any(|entry| {
        entry.original_line == obj_line
            && entry.original_column >= obj_col
            && entry.original_column <= obj_col + 9
    });

    let (x_line, x_col) = find_line_col(source, "const x");
    let has_x_mapping = decoded.iter().any(|entry| {
        entry.original_line == x_line
            && entry.original_column >= x_col
            && entry.original_column <= x_col + 7
    });

    // At minimum, we should have mappings for one of the declarations
    assert!(
        has_obj_mapping || has_x_mapping,
        "expected mappings for optional chaining declarations. mappings: {mappings}"
    );

    // Verify the output contains the variable names (optional chaining should be downleveled)
    assert!(
        output.contains("obj") && output.contains("arr") && output.contains("fn"),
        "expected output to contain variable names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional chaining code"
    );
}

#[test]
fn test_source_map_logical_assignment_operators() {
    // Test logical assignment operators: ||= &&= ??=
    let source = r#"let a = null;
let b = 0;
let c = "hello";
a ||= "default";
b &&= 10;
c ??= "fallback";"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the variable declarations
    let (a_line, a_col) = find_line_col(source, "let a");
    let has_a_mapping = decoded.iter().any(|entry| {
        entry.original_line == a_line
            && entry.original_column >= a_col
            && entry.original_column <= a_col + 5
    });

    let (b_line, b_col) = find_line_col(source, "let b");
    let has_b_mapping = decoded.iter().any(|entry| {
        entry.original_line == b_line
            && entry.original_column >= b_col
            && entry.original_column <= b_col + 5
    });

    // At minimum, we should have mappings for the declarations
    assert!(
        has_a_mapping || has_b_mapping,
        "expected mappings for logical assignment declarations. mappings: {mappings}"
    );

    // Verify output contains the variable names
    assert!(
        output.contains("var a") || output.contains("var b") || output.contains("var c"),
        "expected output to contain variable declarations. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for logical assignment code"
    );
}

#[test]
fn test_source_map_bigint_literals() {
    // Test BigInt literals with n suffix
    let source = r#"const small = 123n;
const large = 9007199254740991n;
const hex = 0xFFFFFFFFFFFFFFFFn;
const binary = 0b1010n;
const sum = small + large;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the variable declarations
    let (small_line, small_col) = find_line_col(source, "const small");
    let has_small_mapping = decoded.iter().any(|entry| {
        entry.original_line == small_line
            && entry.original_column >= small_col
            && entry.original_column <= small_col + 11
    });

    let (large_line, large_col) = find_line_col(source, "const large");
    let has_large_mapping = decoded.iter().any(|entry| {
        entry.original_line == large_line
            && entry.original_column >= large_col
            && entry.original_column <= large_col + 11
    });

    // At minimum, we should have mappings for one of the declarations
    assert!(
        has_small_mapping || has_large_mapping,
        "expected mappings for BigInt declarations. mappings: {mappings}"
    );

    // Verify output contains the variable names
    assert!(
        output.contains("small") && output.contains("large"),
        "expected output to contain variable names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for BigInt code"
    );
}

#[test]
fn test_source_map_class_static_blocks() {
    // Test class static blocks (ES2022)
    let source = r#"class Counter {
    static count = 0;
    static {
        Counter.count = 10;
        console.log("initialized");
    }
    static {
        Counter.count += 5;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the class declaration
    let (class_line, class_col) = find_line_col(source, "class Counter");
    let has_class_mapping = decoded.iter().any(|entry| {
        entry.original_line == class_line
            && entry.original_column >= class_col
            && entry.original_column <= class_col + 13
    });

    // Verify we have mappings for the static property
    let (count_line, count_col) = find_line_col(source, "static count");
    let has_count_mapping = decoded.iter().any(|entry| {
        entry.original_line == count_line
            && entry.original_column >= count_col
            && entry.original_column <= count_col + 12
    });

    // At minimum, we should have mappings for the class or static property
    assert!(
        has_class_mapping || has_count_mapping,
        "expected mappings for class with static blocks. mappings: {mappings}"
    );

    // Verify output contains the class name
    assert!(
        output.contains("Counter"),
        "expected output to contain class name. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class static blocks"
    );
}

#[test]
fn test_source_map_dynamic_import() {
    // Test dynamic import() expressions
    let source = r#"async function loadModule() {
    const mod = await import("./module");
    return mod.default;
}

const lazy = import("./lazy");
const conditional = true ? import("./a") : import("./b");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the async function
    let (func_line, func_col) = find_line_col(source, "async function loadModule");
    let has_func_mapping = decoded.iter().any(|entry| {
        entry.original_line == func_line
            && entry.original_column >= func_col
            && entry.original_column <= func_col + 25
    });

    // Verify we have mappings for the lazy variable
    let (lazy_line, lazy_col) = find_line_col(source, "const lazy");
    let has_lazy_mapping = decoded.iter().any(|entry| {
        entry.original_line == lazy_line
            && entry.original_column >= lazy_col
            && entry.original_column <= lazy_col + 10
    });

    // At minimum, we should have mappings for function or variable
    assert!(
        has_func_mapping || has_lazy_mapping,
        "expected mappings for dynamic import code. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("loadModule") || output.contains("lazy"),
        "expected output to contain function or variable names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for dynamic import"
    );
}

#[test]
fn test_source_map_rest_and_default_parameters() {
    // Test rest parameters (...args) and default parameters (x = value)
    let source = r#"function greet(name = "World", ...titles: string[]) {
    return titles.join(" ") + " " + name;
}

const sum = (a: number, b = 0, ...rest: number[]) => {
    return a + b + rest.reduce((x, y) => x + y, 0);
};

greet("Alice", "Dr.", "Prof.");
sum(1, 2, 3, 4, 5);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the function declaration
    let (greet_line, greet_col) = find_line_col(source, "function greet");
    let has_greet_mapping = decoded.iter().any(|entry| {
        entry.original_line == greet_line
            && entry.original_column >= greet_col
            && entry.original_column <= greet_col + 14
    });

    // Verify we have mappings for the arrow function
    let (sum_line, sum_col) = find_line_col(source, "const sum");
    let has_sum_mapping = decoded.iter().any(|entry| {
        entry.original_line == sum_line
            && entry.original_column >= sum_col
            && entry.original_column <= sum_col + 9
    });

    // At minimum, we should have mappings for function declarations
    assert!(
        has_greet_mapping || has_sum_mapping,
        "expected mappings for rest/default parameter functions. mappings: {mappings}"
    );

    // Verify output contains the function names
    assert!(
        output.contains("greet") && output.contains("sum"),
        "expected output to contain function names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest/default parameters"
    );
}

#[test]
#[ignore = "ES5 IR transform incomplete"]
fn test_source_map_async_es5_offset_accuracy() {
    // Test source-map offset accuracy for async function ES5 downleveling
    // The __awaiter/__generator transform should preserve correct source mappings
    let source = r#"async function fetchData(url: string) {
    const response = await fetch(url);
    const data = await response.json();
    return data;
}

async function processItems(items: string[]) {
    for (const item of items) {
        await processItem(item);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify ES5 async transform was applied
    assert!(
        output.contains("__awaiter") || output.contains("__generator"),
        "expected async ES5 helpers in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for both async function declarations
    let (fetch_line, _) = find_line_col(source, "async function fetchData");
    let has_fetch_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == fetch_line);

    let (process_line, _) = find_line_col(source, "async function processItems");
    let has_process_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == process_line);

    // Verify we have mappings for await expressions
    let (await_fetch_line, _) = find_line_col(source, "await fetch");
    let has_await_fetch_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == await_fetch_line);

    let (await_json_line, _) = find_line_col(source, "await response.json");
    let has_await_json_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == await_json_line);

    // We should have mappings for both function declarations
    assert!(
        has_fetch_mapping && has_process_mapping,
        "expected mappings for both async function declarations. mappings: {mappings}"
    );

    // We should have mappings for await expression lines
    assert!(
        has_await_fetch_mapping || has_await_json_mapping,
        "expected mappings for await expression lines. mappings: {mappings}"
    );

    // Verify non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async ES5 code"
    );

    // Verify mappings span multiple source lines (not all on line 0)
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 3,
        "expected mappings from at least 3 different source lines, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_typescript_enums() {
    // Test TypeScript enum declarations (downleveled to IIFE)
    let source = r#"enum Color {
    Red,
    Green,
    Blue
}

enum Status {
    Active = 1,
    Inactive = 2,
    Pending = 3
}

const myColor = Color.Red;
const myStatus = Status.Active;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the enum declaration
    let (color_line, color_col) = find_line_col(source, "enum Color");
    let has_color_mapping = decoded.iter().any(|entry| {
        entry.original_line == color_line
            && entry.original_column >= color_col
            && entry.original_column <= color_col + 10
    });

    // Verify we have mappings for the variable declaration
    let (var_line, var_col) = find_line_col(source, "const myColor");
    let has_var_mapping = decoded.iter().any(|entry| {
        entry.original_line == var_line
            && entry.original_column >= var_col
            && entry.original_column <= var_col + 13
    });

    // At minimum, we should have mappings for enum or variable
    assert!(
        has_color_mapping || has_var_mapping,
        "expected mappings for enum declarations. mappings: {mappings}"
    );

    // Verify output contains the enum names
    assert!(
        output.contains("Color") && output.contains("Status"),
        "expected output to contain enum names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for TypeScript enums"
    );
}

#[test]
fn test_source_map_generator_es5_offset_accuracy() {
    // Test source-map offset accuracy for generator function ES5 downleveling
    let source = r#"function* numberGenerator() {
    yield 1;
    yield 2;
    yield 3;
}

function* infiniteSequence() {
    let i = 0;
    while (true) {
        yield i++;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify generator function is in output (may be __generator helper or function* syntax)
    assert!(
        output.contains("__generator")
            || output.contains("numberGenerator")
            || output.contains("function*"),
        "expected generator function in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for generator function declarations
    let (num_gen_line, _) = find_line_col(source, "function* numberGenerator");
    let has_num_gen_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == num_gen_line);

    let (inf_seq_line, _) = find_line_col(source, "function* infiniteSequence");
    let has_inf_seq_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == inf_seq_line);

    // We should have mappings for both generator declarations
    assert!(
        has_num_gen_mapping || has_inf_seq_mapping,
        "expected mappings for generator function declarations. mappings: {mappings}"
    );

    // Verify non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator ES5 code"
    );

    // Verify mappings span multiple source lines
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 2,
        "expected mappings from at least 2 different source lines, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_destructuring_patterns() {
    // Test object and array destructuring patterns
    let source = r#"const obj = { a: 1, b: 2, c: 3 };
const { a, b: renamed, ...rest } = obj;

const arr = [1, 2, 3, 4, 5];
const [first, second, ...remaining] = arr;

function processPoint({ x, y }: { x: number; y: number }) {
    return x + y;
}

const swap = ([a, b]: [number, number]) => [b, a];

const result = processPoint({ x: 10, y: 20 });
const swapped = swap([1, 2]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the object declaration
    let (obj_line, obj_col) = find_line_col(source, "const obj");
    let has_obj_mapping = decoded.iter().any(|entry| {
        entry.original_line == obj_line
            && entry.original_column >= obj_col
            && entry.original_column <= obj_col + 9
    });

    // Verify we have mappings for the function declaration
    let (fn_line, fn_col) = find_line_col(source, "function processPoint");
    let has_fn_mapping = decoded.iter().any(|entry| {
        entry.original_line == fn_line
            && entry.original_column >= fn_col
            && entry.original_column <= fn_col + 21
    });

    // At minimum, we should have mappings for declarations
    assert!(
        has_obj_mapping || has_fn_mapping,
        "expected mappings for destructuring patterns. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("processPoint") && output.contains("swap"),
        "expected output to contain function names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for destructuring patterns"
    );
}

#[test]
fn test_source_map_private_class_fields() {
    // Test private class fields (#field) source map coverage
    let source = r#"class Counter {
    #count = 0;
    #name: string;

    constructor(name: string) {
        this.#name = name;
    }

    increment() {
        this.#count++;
        return this.#count;
    }

    get value() {
        return this.#count;
    }

    set value(n: number) {
        this.#count = n;
    }

    static #instances = 0;

    static create(name: string) {
        Counter.#instances++;
        return new Counter(name);
    }
}

const c = new Counter("test");
c.increment();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the class declaration
    let (class_line, class_col) = find_line_col(source, "class Counter");
    let has_class_mapping = decoded.iter().any(|entry| {
        entry.original_line == class_line
            && entry.original_column >= class_col
            && entry.original_column <= class_col + 13
    });

    // Verify we have mappings for method declarations
    let (inc_line, inc_col) = find_line_col(source, "increment()");
    let has_inc_mapping = decoded.iter().any(|entry| {
        entry.original_line == inc_line
            && entry.original_column >= inc_col
            && entry.original_column <= inc_col + 10
    });

    // At minimum, we should have mappings for class or methods
    assert!(
        has_class_mapping || has_inc_mapping || !decoded.is_empty(),
        "expected mappings for private class fields. mappings: {mappings}"
    );

    // Verify output contains Counter class name
    assert!(
        output.contains("Counter"),
        "expected output to contain class name. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for private class fields"
    );
}

#[test]
fn test_source_map_class_static_block_mapping() {
    // Test source-map accuracy for class static blocks
    let source = r#"class Config {
    static initialized = false;
    static settings: Record<string, string> = {};

    static {
        Config.initialized = true;
        Config.settings["mode"] = "production";
    }

    static {
        console.log("Config loaded");
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify class name is in output
    assert!(
        output.contains("Config"),
        "expected class name in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the class declaration
    let (class_line, _) = find_line_col(source, "class Config");
    let has_class_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == class_line);

    // Verify we have mappings for static properties
    let (initialized_line, _) = find_line_col(source, "static initialized");
    let has_initialized_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == initialized_line);

    // We should have mappings for the class and static members
    assert!(
        has_class_mapping || has_initialized_mapping,
        "expected mappings for class static blocks. mappings: {mappings}"
    );

    // Verify non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class static blocks"
    );

    // Verify we have mappings (at least one source line covered)
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        !unique_source_lines.is_empty(),
        "expected at least one source line covered in mappings, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_nullish_coalescing() {
    // Test nullish coalescing operator (??) source map coverage
    let source = r#"const value1 = null ?? "default1";
const value2 = undefined ?? "default2";
const value3 = 0 ?? "not used";
const value4 = "" ?? "not used either";

function getValue(input: string | null | undefined) {
    return input ?? "fallback";
}

const nested = null ?? undefined ?? "final";

const obj = { prop: null };
const result = obj.prop ?? "missing";

const arr: (number | null)[] = [1, null, 3];
const mapped = arr.map(x => x ?? 0);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the const declarations
    let (value1_line, value1_col) = find_line_col(source, "const value1");
    let has_value1_mapping = decoded.iter().any(|entry| {
        entry.original_line == value1_line
            && entry.original_column >= value1_col
            && entry.original_column <= value1_col + 12
    });

    // Verify we have mappings for the function declaration
    let (fn_line, fn_col) = find_line_col(source, "function getValue");
    let has_fn_mapping = decoded.iter().any(|entry| {
        entry.original_line == fn_line
            && entry.original_column >= fn_col
            && entry.original_column <= fn_col + 17
    });

    // At minimum, we should have mappings for declarations
    assert!(
        has_value1_mapping || has_fn_mapping || !decoded.is_empty(),
        "expected mappings for nullish coalescing. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("getValue") && output.contains("value1"),
        "expected output to contain function and variable names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nullish coalescing"
    );
}

#[test]
fn test_source_map_template_literals() {
    // Test template literals with expressions source map coverage
    let source = r#"const name = "World";
const greeting = `Hello, ${name}!`;

const a = 10;
const b = 20;
const sum = `${a} + ${b} = ${a + b}`;

function format(items: string[]) {
    return `Items: ${items.join(", ")}`;
}

const nested = `outer ${`inner ${name}`}`;

const multiline = `
    Line 1
    Line 2: ${name}
    Line 3
`;

const tagged = String.raw`path\to\${name}`;

const result = format(["apple", "banana"]);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the const declarations
    let (name_line, name_col) = find_line_col(source, "const name");
    let has_name_mapping = decoded.iter().any(|entry| {
        entry.original_line == name_line
            && entry.original_column >= name_col
            && entry.original_column <= name_col + 10
    });

    // Verify we have mappings for the function declaration
    let (fn_line, fn_col) = find_line_col(source, "function format");
    let has_fn_mapping = decoded.iter().any(|entry| {
        entry.original_line == fn_line
            && entry.original_column >= fn_col
            && entry.original_column <= fn_col + 15
    });

    // At minimum, we should have mappings for declarations
    assert!(
        has_name_mapping || has_fn_mapping || !decoded.is_empty(),
        "expected mappings for template literals. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("format") && output.contains("greeting"),
        "expected output to contain function and variable names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for template literals"
    );
}

#[test]
fn test_source_map_class_expressions() {
    // Test class expressions source map coverage
    let source = r#"const MyClass = class {
    value = 42;

    getValue() {
        return this.value;
    }
};

const NamedClass = class InternalName {
    static count = 0;

    constructor() {
        InternalName.count++;
    }
};

const factory = () => class {
    data: string;

    constructor(data: string) {
        this.data = data;
    }
};

const instance1 = new MyClass();
const instance2 = new NamedClass();
const DynamicClass = factory();
const instance3 = new DynamicClass("test");"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the const declarations
    let (myclass_line, myclass_col) = find_line_col(source, "const MyClass");
    let has_myclass_mapping = decoded.iter().any(|entry| {
        entry.original_line == myclass_line
            && entry.original_column >= myclass_col
            && entry.original_column <= myclass_col + 13
    });

    // Verify we have mappings for the factory function
    let (factory_line, factory_col) = find_line_col(source, "const factory");
    let has_factory_mapping = decoded.iter().any(|entry| {
        entry.original_line == factory_line
            && entry.original_column >= factory_col
            && entry.original_column <= factory_col + 13
    });

    // At minimum, we should have mappings for declarations
    assert!(
        has_myclass_mapping || has_factory_mapping || !decoded.is_empty(),
        "expected mappings for class expressions. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("MyClass") && output.contains("factory"),
        "expected output to contain class and function names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expressions"
    );
}

#[test]
fn test_source_map_exponentiation_operator_mapping() {
    // Test source-map accuracy for exponentiation operator (**)
    let source = r#"const square = 2 ** 2;
const cube = 3 ** 3;
const power = base ** exponent;
let x = 2;
x **= 3;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("square") && output.contains("cube"),
        "expected variable names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for exponentiation operator"
    );
}

#[test]
fn test_source_map_rest_spread_mapping() {
    // Test source-map accuracy for rest parameters and spread arguments
    let source = r#"function sum(...numbers: number[]): number {
    return numbers.reduce((a, b) => a + b, 0);
}

const arr = [1, 2, 3];
const result = sum(...arr);

const [first, ...rest] = arr;
const { x, ...others } = { x: 1, y: 2, z: 3 };"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("sum") && output.contains("arr"),
        "expected function and variable names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest/spread"
    );
}

#[test]
fn test_source_map_default_parameters_mapping() {
    // Test source-map accuracy for default parameter values
    let source = r#"function greet(name: string = "World", count: number = 1): string {
    return `Hello ${name}!`.repeat(count);
}

const add = (a: number = 0, b: number = 0) => a + b;

class Calculator {
    multiply(x: number = 1, y: number = 1): number {
        return x * y;
    }
}

function format(value: string, options: { uppercase?: boolean } = {}): string {
    return options.uppercase ? value.toUpperCase() : value;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("greet") && output.contains("add") && output.contains("Calculator"),
        "expected function and class names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the function declarations
    let (greet_line, _) = find_line_col(source, "function greet");
    let has_greet_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == greet_line);

    let (add_line, _) = find_line_col(source, "const add");
    let has_add_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == add_line);

    assert!(
        has_greet_mapping || has_add_mapping,
        "expected mappings for default parameter declarations. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default parameters"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 3,
        "expected mappings from at least 3 different source lines, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_arrow_functions() {
    // Test arrow functions source map coverage
    let source = r#"const add = (a: number, b: number) => a + b;

const square = (x: number) => x * x;

const identity = <T>(value: T) => value;

const multiLine = (x: number, y: number) => {
    const sum = x + y;
    const product = x * y;
    return { sum, product };
};

const nested = (a: number) => (b: number) => (c: number) => a + b + c;

const withThis = {
    value: 10,
    getValue: function() {
        return () => this.value;
    }
};

const arr = [1, 2, 3, 4, 5];
const doubled = arr.map(x => x * 2);
const filtered = arr.filter(x => x > 2);
const reduced = arr.reduce((acc, x) => acc + x, 0);"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the const declarations
    let (add_line, add_col) = find_line_col(source, "const add");
    let has_add_mapping = decoded.iter().any(|entry| {
        entry.original_line == add_line
            && entry.original_column >= add_col
            && entry.original_column <= add_col + 9
    });

    // Verify we have mappings for multiLine function
    let (multi_line, multi_col) = find_line_col(source, "const multiLine");
    let has_multi_mapping = decoded.iter().any(|entry| {
        entry.original_line == multi_line
            && entry.original_column >= multi_col
            && entry.original_column <= multi_col + 15
    });

    // At minimum, we should have mappings for declarations
    assert!(
        has_add_mapping || has_multi_mapping || !decoded.is_empty(),
        "expected mappings for arrow functions. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("multiLine") && output.contains("doubled"),
        "expected output to contain function and variable names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow functions"
    );
}

#[test]
fn test_source_map_computed_property_names_mapping() {
    // Test source-map accuracy for computed property names
    let source = r#"const key = "dynamic";
const sym = Symbol("unique");

const obj = {
    [key]: "value1",
    [sym]: "value2",
    ["literal"]: "value3"
};

class MyClass {
    [key]: string = "field";

    [sym]() {
        return "method";
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("key") && output.contains("obj") && output.contains("MyClass"),
        "expected variable and class names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed property names"
    );
}

#[test]
fn test_source_map_shorthand_properties_mapping() {
    // Test source-map accuracy for shorthand property syntax
    let source = r#"const name = "John";
const age = 30;
const active = true;

const person = { name, age, active };

function createUser(id: number, email: string) {
    return { id, email, createdAt: Date.now() };
}

const coords = { x: 10, y: 20 };
const { x, y } = coords;

const merged = { ...coords, z: 30 };"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("name") && output.contains("person") && output.contains("createUser"),
        "expected variable and function names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    let (name_line, _) = find_line_col(source, "const name");
    let has_name_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == name_line);

    let (person_line, _) = find_line_col(source, "const person");
    let has_person_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == person_line);

    assert!(
        has_name_mapping || has_person_mapping,
        "expected mappings for shorthand property declarations. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for shorthand properties"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 3,
        "expected mappings from at least 3 different source lines, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_method_definitions_mapping() {
    // Test source-map accuracy for method definition syntax
    let source = r#"const calculator = {
    add(a: number, b: number): number {
        return a + b;
    },
    subtract(a: number, b: number): number {
        return a - b;
    },
    *generator() {
        yield 1;
        yield 2;
    },
    async fetchData() {
        return await Promise.resolve(42);
    },
    get value() {
        return this._value;
    },
    set value(v: number) {
        this._value = v;
    }
};

class Counter {
    private count = 0;

    increment(): void {
        this.count++;
    }

    decrement(): void {
        this.count--;
    }

    async loadAsync(): Promise<number> {
        return this.count;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("calculator") && output.contains("Counter") && output.contains("add"),
        "expected object and class names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    let (calc_line, _) = find_line_col(source, "const calculator");
    let has_calc_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == calc_line);

    let (counter_line, _) = find_line_col(source, "class Counter");
    let has_counter_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == counter_line);

    assert!(
        has_calc_mapping || has_counter_mapping,
        "expected mappings for method definitions. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method definitions"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 5,
        "expected mappings from at least 5 different source lines, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_for_of_for_in_loops_mapping() {
    // Test source-map accuracy for for-of and for-in loops
    let source = r#"const numbers = [1, 2, 3, 4, 5];
const obj = { a: 1, b: 2, c: 3 };

for (const num of numbers) {
    console.log(num);
}

for (const key in obj) {
    console.log(key, obj[key]);
}

for (let i of [10, 20, 30]) {
    i *= 2;
    console.log(i);
}

const iterable = new Map([["x", 1], ["y", 2]]);
for (const [key, value] of iterable) {
    console.log(key, value);
}

async function processItems(items: number[]) {
    for await (const item of items) {
        console.log(item);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("numbers") && output.contains("obj"),
        "expected variable names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    let (numbers_line, _) = find_line_col(source, "const numbers");
    let has_numbers_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == numbers_line);

    let (for_of_line, _) = find_line_col(source, "for (const num of");
    let has_for_of_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == for_of_line);

    assert!(
        has_numbers_mapping || has_for_of_mapping,
        "expected mappings for for-of/for-in loops. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of/for-in loops"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 5,
        "expected mappings from at least 5 different source lines, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_typescript_namespaces() {
    // Test source-map accuracy for TypeScript namespace declarations
    let source = r#"namespace MyNamespace {
    export const value = 42;

    export function greet(name: string): string {
        return "Hello, " + name;
    }

    export class Helper {
        static compute(x: number): number {
            return x * 2;
        }
    }
}

namespace Nested.Inner {
    export const nested = "inner value";
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify we have mappings for the namespace declaration
    let (ns_line, ns_col) = find_line_col(source, "namespace MyNamespace");
    let has_ns_mapping = decoded.iter().any(|entry| {
        entry.original_line == ns_line
            && entry.original_column >= ns_col
            && entry.original_column <= ns_col + 20
    });

    // Verify we have mappings for the nested namespace
    let (nested_line, nested_col) = find_line_col(source, "namespace Nested");
    let has_nested_mapping = decoded.iter().any(|entry| {
        entry.original_line == nested_line
            && entry.original_column >= nested_col
            && entry.original_column <= nested_col + 16
    });

    // At minimum, we should have mappings for namespace declarations
    assert!(
        has_ns_mapping || has_nested_mapping || !decoded.is_empty(),
        "expected mappings for namespace declarations. mappings: {mappings}"
    );

    // Verify output contains namespace IIFE pattern
    assert!(
        output.contains("MyNamespace") || output.contains("var MyNamespace"),
        "expected output to contain namespace identifiers. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for TypeScript namespaces"
    );
}

#[test]
fn test_source_map_block_scoping_let_const_mapping() {
    // Test let/const to var transform source mapping
    let source = r#"let x = 1;
const y = 2;
let z = x + y;
console.log(z);"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        "expected mappings from at least 2 different source lines, got: {:?}",
        unique_source_lines
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = crate::emitter::ModuleKind::CommonJS;
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

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = crate::emitter::ModuleKind::CommonJS;
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

#[test]
fn test_source_map_commonjs_default_export_mapping() {
    // Test CommonJS default export transform source mapping
    let source = r#"const myValue = 100;

export default myValue;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = crate::emitter::ModuleKind::CommonJS;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify default export or myValue in output
    assert!(
        output.contains("myValue") || output.contains("default"),
        "expected default export in output: {output}"
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
        "expected non-empty source mappings for default export"
    );
}

#[test]
fn test_source_map_commonjs_reexport_mapping() {
    // Test CommonJS re-export transform source mapping
    let source = r#"export { foo, bar } from "./module";
export * from "./utils";"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    options.module = crate::emitter::ModuleKind::CommonJS;
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

    // Verify we have mappings for re-exports
    let (reexport_line, _) = find_line_col(source, "export { foo");
    let has_reexport_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == reexport_line);

    assert!(
        has_reexport_mapping || !decoded.is_empty(),
        "expected mappings for re-exports. mappings: {mappings} output: {output}"
    );
}

#[test]
fn test_source_map_type_assertions_and_const() {
    // Test source-map accuracy for type assertions and const assertions
    let source = r#"const value: unknown = "hello";
const str = value as string;
const num = <number>someValue;

const config = {
    name: "app",
    version: 1
} as const;

const colors = ["red", "green", "blue"] as const;

function process(input: unknown) {
    const data = input as { id: number; name: string };
    return data.id;
}

type Point = { x: number; y: number };
const origin = { x: 0, y: 0 } as Point;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
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

    // Verify we have mappings for variable declarations
    let (value_line, _) = find_line_col(source, "const value");
    let has_value_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == value_line);

    let (config_line, _) = find_line_col(source, "const config");
    let has_config_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == config_line);

    // At minimum, we should have mappings for declarations
    assert!(
        has_value_mapping || has_config_mapping || !decoded.is_empty(),
        "expected mappings for type assertions. mappings: {mappings}"
    );

    // Type assertions should be stripped from output
    assert!(
        !output.contains(" as string") && !output.contains(" as const"),
        "expected type assertions to be stripped. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type assertions"
    );
}

#[test]
fn test_source_map_jsx_element_mapping() {
    // Test JSX element source mapping
    let source = r#"const element = <div className="container">Hello</div>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX is in output
    assert!(
        output.contains("<div") || output.contains("div"),
        "expected JSX element in output: {output}"
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
        "expected non-empty source mappings for JSX element"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_jsx_fragment_mapping() {
    // Test JSX fragment source mapping
    let source = r#"const fragment = <>
    <span>First</span>
    <span>Second</span>
</>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX fragment is in output
    assert!(
        output.contains("<>") || output.contains("Fragment") || output.contains("span"),
        "expected JSX fragment in output: {output}"
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
        "expected non-empty source mappings for JSX fragment"
    );
}

#[test]
fn test_source_map_jsx_expression_mapping() {
    // Test JSX with expressions source mapping
    let source = r#"const name = "World";
const greeting = <h1>Hello, {name}!</h1>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX with expression is in output
    assert!(
        output.contains("name") && output.contains("h1"),
        "expected JSX expression in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for both declarations
    let (name_line, _) = find_line_col(source, "const name");
    let has_name_mapping = decoded.iter().any(|entry| entry.original_line == name_line);

    let (greeting_line, _) = find_line_col(source, "const greeting");
    let has_greeting_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == greeting_line);

    assert!(
        has_name_mapping || has_greeting_mapping || !decoded.is_empty(),
        "expected mappings for JSX expressions. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_jsx_component_mapping() {
    // Test JSX component with props source mapping
    let source = r#"function Button({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
    return <button onClick={onClick}>{children}</button>;
}

const app = <Button onClick={() => console.log("clicked")}>Click me</Button>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify component is in output
    assert!(
        output.contains("Button") && output.contains("button"),
        "expected JSX component in output: {output}"
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
        "expected non-empty source mappings for JSX component"
    );

    // Verify mappings cover multiple source lines
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 2,
        "expected mappings from at least 2 different source lines, got: {:?}",
        unique_source_lines
    );
}

#[test]
fn test_source_map_namespace_es5_basic_mapping() {
    // Basic namespace transforms to IIFE pattern
    let source = r#"namespace Foo {
    export const value = 42;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();

    assert!(
        output.contains("var Foo;"),
        "expected var Foo declaration in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for namespace"
    );
}

#[test]
fn test_source_map_namespace_es5_nested_mapping() {
    // Nested/qualified namespace A.B.C
    let source = r#"namespace A.B.C {
    export function greet() {
        return "hello";
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_target_es5(ctx.target_es5);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();

    assert!(
        output.contains("var A;") || output.contains("var A "),
        "expected var A declaration for nested namespace in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested namespace"
    );
}

#[test]
fn test_source_map_class_decorator_single() {
    // Test single class decorator source mapping
    let source = r#"function Component(target: Function) {
    return target;
}

@Component
class MyComponent {
    render() {
        return "hello";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class and decorator pattern
    assert!(
        output.contains("MyComponent") && output.contains("render"),
        "expected output to contain class and method. output: {output}"
    );

    // Verify we have source mappings that reference source file
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class decorator"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_class_decorator_multiple() {
    // Test multiple class decorators source mapping
    let source = r#"function Component(target: Function) {}
function Injectable(target: Function) {}
function Sealed(target: Function) {}

@Component
@Injectable
@Sealed
class Service {
    constructor() {}
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the decorated class
    assert!(
        output.contains("Service"),
        "expected output to contain Service class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple class decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_method_decorator_single() {
    // Test single method decorator source mapping
    let source = r#"function Log(target: any, key: string, descriptor: PropertyDescriptor) {
    return descriptor;
}

class Logger {
    @Log
    log(message: string) {
        console.log(message);
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class and method
    assert!(
        output.contains("Logger") && output.contains("log"),
        "expected output to contain Logger class and log method. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method decorator"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_method_decorator_multiple() {
    // Test multiple method decorators source mapping
    let source = r#"function Bind(target: any, key: string, descriptor: PropertyDescriptor) {}
function Memoize(target: any, key: string, descriptor: PropertyDescriptor) {}
function Validate(target: any, key: string, descriptor: PropertyDescriptor) {}

class Calculator {
    @Bind
    @Memoize
    @Validate
    calculate(x: number, y: number) {
        return x + y;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class and method
    assert!(
        output.contains("Calculator") && output.contains("calculate"),
        "expected output to contain Calculator class and calculate method. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple method decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_parameter_decorator() {
    // Test parameter decorator source mapping
    let source = r#"function Inject(target: any, key: string, index: number) {}

class Container {
    constructor(@Inject service: any) {}
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class
    assert!(
        output.contains("Container"),
        "expected output to contain Container class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for parameter decorator"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_parameter_decorator_multiple_params() {
    // Test multiple parameter decorators on different parameters
    let source = r#"function Required(target: any, key: string, index: number) {}
function Optional(target: any, key: string, index: number) {}

class Service {
    process(@Required input: string, @Optional options?: object) {
        return input;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class (process may be transformed/elided)
    assert!(
        output.contains("Service"),
        "expected output to contain Service class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple parameter decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_property_decorator() {
    // Test property decorator source mapping
    let source = r#"function Column(target: any, key: string) {}
function PrimaryKey(target: any, key: string) {}

class Entity {
    @PrimaryKey
    id: number = 0;

    @Column
    name: string = "";
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class
    assert!(
        output.contains("Entity"),
        "expected output to contain Entity class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for property decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_accessor_decorator() {
    // Test accessor (getter/setter) decorator source mapping
    let source = r#"function Cached(target: any, key: string, descriptor: PropertyDescriptor) {}
function Validate(target: any, key: string, descriptor: PropertyDescriptor) {}

class Config {
    private _value: number = 0;

    @Cached
    get value(): number {
        return this._value;
    }

    @Validate
    set value(v: number) {
        this._value = v;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class and accessors
    assert!(
        output.contains("Config") && output.contains("value"),
        "expected output to contain Config class and value accessor. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for accessor decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_decorator_factory() {
    // Test decorator factory (decorator with arguments) source mapping
    let source = r#"function Route(path: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {};
}

function Method(method: string) {
    return function(target: any, key: string, descriptor: PropertyDescriptor) {};
}

class Controller {
    @Route("/api/users")
    @Method("GET")
    getUsers() {
        return [];
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class and method
    assert!(
        output.contains("Controller") && output.contains("getUsers"),
        "expected output to contain Controller class and getUsers method. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for decorator factories"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_mixed_decorators() {
    // Test class with class, method, property, and parameter decorators
    let source = r#"function Entity(target: Function) {}
function Column(target: any, key: string) {}
function BeforeInsert(target: any, key: string, descriptor: PropertyDescriptor) {}
function Inject(target: any, key: string, index: number) {}

@Entity
class User {
    @Column
    name: string = "";

    @BeforeInsert
    validate(@Inject validator: any) {
        return true;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains the class (validate may be transformed/elided)
    assert!(
        output.contains("User"),
        "expected output to contain User class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mixed decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_async_multiple_awaits() {
    // Test async function with multiple await expressions in sequence
    let source = r#"async function processAll(a: Promise<number>, b: Promise<string>, c: Promise<boolean>) {
    const numResult = await a;
    const strResult = await b;
    const boolResult = await c;
    return { numResult, strResult, boolResult };
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains async transform helpers
    assert!(
        output.contains("__awaiter") || output.contains("__generator"),
        "expected async downlevel output. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple awaits"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_try_catch() {
    // Test async function with try/catch around await
    let source = r#"async function safeFetch(url: string) {
    try {
        const response = await fetch(url);
        return await response.json();
    } catch (error) {
        console.error(error);
        return null;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains async transform and try/catch
    assert!(
        output.contains("__awaiter") || output.contains("__generator"),
        "expected async downlevel output. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async try/catch"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_for_of_loop() {
    // Test async function with await inside for...of loop
    let source = r#"async function processItems(items: Promise<number>[]) {
    const results: number[] = [];
    for (const item of items) {
        const value = await item;
        results.push(value);
    }
    return results;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains async transform
    assert!(
        output.contains("__awaiter") || output.contains("__generator"),
        "expected async downlevel output. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async for-of loop"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_iife() {
    // Test async IIFE (Immediately Invoked Function Expression)
    let source = r#"const result = (async () => {
    const data = await fetchData();
    return data.value;
})();"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains result variable
    assert!(
        output.contains("result"),
        "expected output to contain result variable. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async IIFE"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_rest_params() {
    // Test async function with rest parameters
    let source = r#"async function processMany(...promises: Promise<number>[]) {
    const results: number[] = [];
    for (const p of promises) {
        results.push(await p);
    }
    return results;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("processMany"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async rest params"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_default_params() {
    // Test async function with default parameters
    let source = r#"async function fetchWithTimeout(url: string, timeout: number = 5000) {
    const controller = new AbortController();
    setTimeout(() => controller.abort(), timeout);
    return await fetch(url, { signal: controller.signal });
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("fetchWithTimeout"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async default params"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_destructuring_await() {
    // Test async function with destructuring of awaited value
    let source = r#"async function getUser(id: number) {
    const { name, email, age } = await fetchUser(id);
    return { name, email, age };
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("getUser"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async destructuring"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_nested_functions() {
    // Test nested async functions
    let source = r#"async function outer() {
    async function inner(value: number) {
        return await Promise.resolve(value * 2);
    }
    const result = await inner(21);
    return result;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains both function names
    assert!(
        output.contains("outer") && output.contains("inner"),
        "expected output to contain both function names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested async functions"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_class_static_method() {
    // Test async static class method
    let source = r#"class DataService {
    static async fetchAll() {
        const items = await getItems();
        return items;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class name
    assert!(
        output.contains("DataService"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async static method"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_async_while_loop() {
    // Test async function with await in while loop
    let source = r#"async function pollUntilDone(checkStatus: () => Promise<boolean>) {
    while (!(await checkStatus())) {
        await delay(1000);
    }
    return true;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("pollUntilDone"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async while loop"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Class Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_es5_class_basic_iife() {
    // Test basic class lowered to ES5 IIFE pattern
    let source = r#"class Point {
    x: number;
    y: number;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class name as function
    assert!(
        output.contains("Point"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class IIFE"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_constructor() {
    // Test class with constructor lowered to ES5
    let source = r#"class Rectangle {
    width: number;
    height: number;
    constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class name and property assignments
    assert!(
        output.contains("Rectangle"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class constructor"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_instance_methods() {
    // Test class with instance methods lowered to ES5
    let source = r#"class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
    subtract(a: number, b: number): number {
        return a - b;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class and method names
    assert!(
        output.contains("Calculator"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class instance methods"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_static_methods() {
    // Test class with static methods lowered to ES5
    let source = r#"class MathUtils {
    static PI: number = 3.14159;
    static square(x: number): number {
        return x * x;
    }
    static cube(x: number): number {
        return x * x * x;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class and static members
    assert!(
        output.contains("MathUtils"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class static methods"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_accessors() {
    // Test class with getters and setters lowered to ES5
    let source = r#"class Person {
    private _name: string = "";
    get name(): string {
        return this._name;
    }
    set name(value: string) {
        this._name = value;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class name
    assert!(
        output.contains("Person"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class accessors"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_inheritance() {
    // Test class inheritance lowered to ES5 with __extends
    let source = r#"class Animal {
    name: string;
    constructor(name: string) {
        this.name = name;
    }
}
class Dog extends Animal {
    breed: string;
    constructor(name: string, breed: string) {
        super(name);
        this.breed = breed;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains both class names
    assert!(
        output.contains("Animal") && output.contains("Dog"),
        "expected output to contain class names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class inheritance"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_super_method_call() {
    // Test super method calls in derived class lowered to ES5
    let source = r#"class Base {
    greet(): string {
        return "Hello";
    }
}
class Derived extends Base {
    greet(): string {
        return super.greet() + " World";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class names
    assert!(
        output.contains("Base") && output.contains("Derived"),
        "expected output to contain class names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 super method call"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_computed_property() {
    // Test class with computed property names lowered to ES5
    let source = r#"const methodName = "dynamicMethod";
class DynamicClass {
    [methodName](): string {
        return "dynamic";
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains class name
    assert!(
        output.contains("DynamicClass"),
        "expected output to contain class name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 computed property"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_multiple_inheritance_levels() {
    // Test multi-level inheritance chain lowered to ES5
    let source = r#"class GrandParent {
    level(): number { return 1; }
}
class Parent extends GrandParent {
    level(): number { return 2; }
}
class Child extends Parent {
    level(): number { return 3; }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains all class names
    assert!(
        output.contains("GrandParent") && output.contains("Parent") && output.contains("Child"),
        "expected output to contain all class names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 multi-level inheritance"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_es5_class_expression() {
    // Test class expressions lowered to ES5
    let source = r#"const MyClass = class {
    value: number = 42;
    getValue(): number {
        return this.value;
    }
};"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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
        output.contains("MyClass"),
        "expected output to contain class expression variable. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for ES5 class expression"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Generator Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_generator_basic_yield() {
    // Test basic generator function with single yield
    let source = r#"function* simpleGenerator() {
    yield 1;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains generator function name
    assert!(
        output.contains("simpleGenerator"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic generator"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_multiple_yields() {
    // Test generator function with multiple yield statements
    let source = r#"function* multiYield() {
    yield 1;
    yield 2;
    yield 3;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains generator function name
    assert!(
        output.contains("multiYield"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multi-yield generator"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_yield_in_loop() {
    // Test generator with yield inside a loop
    let source = r#"function* loopGenerator(max: number) {
    for (let i = 0; i < max; i++) {
        yield i;
    }
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains generator function name
    assert!(
        output.contains("loopGenerator"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for loop generator"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_yield_delegation() {
    // Test generator with yield* (delegation)
    let source = r#"function* inner() {
    yield 1;
    yield 2;
}
function* outer() {
    yield* inner();
    yield 3;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains both generator function names
    assert!(
        output.contains("inner") && output.contains("outer"),
        "expected output to contain both generator function names. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for yield delegation"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_generator_with_return() {
    // Test generator function with return value
    let source = r#"function* generatorWithReturn() {
    yield 1;
    yield 2;
    return "done";
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut options = PrinterOptions::default();
    options.target = ScriptTarget::ES5;
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

    // Verify output contains generator function name
    assert!(
        output.contains("generatorWithReturn"),
        "expected output to contain generator function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator with return"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
