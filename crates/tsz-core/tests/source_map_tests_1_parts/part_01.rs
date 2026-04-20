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

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
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
        "Should be v3 source map: {json}"
    );
    assert!(
        json.contains("\"file\":\"output.js\"") || json.contains("\"file\": \"output.js\""),
        "Should have file: {json}"
    );
    assert!(
        json.contains("\"sources\":[\"input.ts\"]") || json.contains("\"sources\": [\"input.ts\"]"),
        "Should have sources: {json}"
    );
    assert!(
        json.contains("\"mappings\""),
        "Should have mappings: {json}"
    );
}

#[test]
fn test_source_map_with_content() {
    let mut generator = SourceMapGenerator::new("output.js".to_string());
    let _ = generator.add_source_with_content("input.ts".to_string(), "const x = 1;".to_string());

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
        "Should have names: {json}"
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
    let _ = generator.add_source("input.ts".to_string());
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

