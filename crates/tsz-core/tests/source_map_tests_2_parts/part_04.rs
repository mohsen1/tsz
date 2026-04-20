#[test]
fn test_source_map_destructuring_nested_array() {
    // Test nested array destructuring
    let source = r#"const matrix = [[1, 2], [3, 4], [5, 6]];
const [[a, b], [c, d]] = matrix;
console.log(a, b, c, d);"#;

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
        output.contains("matrix"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested array destructuring"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_object_with_defaults() {
    // Test object destructuring with default values
    let source = r#"const config = { host: "localhost" };
const { host, port = 3000, protocol = "http" } = config;
console.log(host, port, protocol);"#;

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
        output.contains("config"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object destructuring with defaults"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_array_with_defaults() {
    // Test array destructuring with default values
    let source = r#"const partial = [1];
const [x = 0, y = 0, z = 0] = partial;
console.log(x, y, z);"#;

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
        output.contains("partial"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array destructuring with defaults"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_function_params() {
    // Test destructuring in function parameters
    let source = r#"function processUser({ name, age }: { name: string, age: number }) {
    console.log(name, age);
}
function processCoords([x, y]: [number, number]) {
    console.log(x, y);
}
processUser({ name: "John", age: 30 });
processCoords([10, 20]);"#;

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

    // Verify output contains function names
    assert!(
        output.contains("processUser"),
        "expected output to contain function name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for destructuring in function params"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_destructuring_mixed() {
    // Test mixed object and array destructuring
    let source = r#"const data = {
    items: [1, 2, 3],
    meta: { count: 3, page: 1 }
};
const { items: [first, second], meta: { count } } = data;
console.log(first, second, count);"#;

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
        output.contains("data"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for mixed destructuring"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Optional Chaining Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_optional_chaining_property_access() {
    // Test optional chaining property access
    let source = r#"const user = { name: "John", address: { city: "NYC" } };
const city = user?.address?.city;
console.log(city);"#;

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
        output.contains("user"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional chaining property access"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_optional_chaining_method_call() {
    // Test optional chaining method call
    let source = r#"const obj = {
    greet: function(name: string) { return "Hello " + name; }
};
const result = obj?.greet?.("World");
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
        output.contains("obj"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional chaining method call"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_optional_chaining_element_access() {
    // Test optional chaining element access
    let source = r#"const arr = [1, 2, 3];
const item = arr?.[0];
const obj = { items: ["a", "b", "c"] };
const val = obj?.items?.[1];
console.log(item, val);"#;

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
        output.contains("arr"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional chaining element access"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_optional_chaining_nested() {
    // Test deeply nested optional chaining
    let source = r#"const data = {
    level1: {
        level2: {
            level3: {
                value: 42
            }
        }
    }
};
const result = data?.level1?.level2?.level3?.value;
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
        output.contains("data"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested optional chaining"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_optional_chaining_with_nullish_coalescing() {
    // Test optional chaining with nullish coalescing
    let source = r#"const config = { timeout: null };
const timeout = config?.timeout ?? 3000;
const host = config?.host ?? "localhost";
console.log(timeout, host);"#;

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
        output.contains("config"),
        "expected output to contain variable name. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for optional chaining with nullish coalescing"
    );

    // Verify at least some mappings reference the source file
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

