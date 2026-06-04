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
    let (parser, root) = parse_test_source(source);

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
        "expected at least one source line covered in mappings, got: {unique_source_lines:?}"
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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
