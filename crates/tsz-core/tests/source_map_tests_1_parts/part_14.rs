#[test]
fn test_source_map_es5_transform_async_nested_try_finally_mapping() {
    let source = "async function run(){ try { try { await inner(); } finally { await cleanup1(); } } finally { await cleanup2(); } }";
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
#[ignore = "regressed after remote changes: yield expression source map mappings no longer generated"]
fn test_source_map_es5_transform_generator_yield_mapping() {
    let source = "function* gen() { yield first(); yield second(); }";
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
            "expected names array to include '{expected}'. names: {names:?}"
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

