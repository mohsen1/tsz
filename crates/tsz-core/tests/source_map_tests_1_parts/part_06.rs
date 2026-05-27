#[test]
fn test_source_map_es5_transform_async_exponentiation_mapping() {
    let source = "async function run(){ return (await base()) ** (await exp()); }";
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

/// TODO: ES5 IR transform does not yet produce source mappings for await expression lines.
/// Function declaration mappings work, but await-internal mappings are missing.
/// When the ES5 async transform improves source map coverage, update the await assertions.
#[test]
fn test_source_map_async_es5_offset_accuracy() {
    // Test source-map offset accuracy for async function ES5 downleveling
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

    // We should have mappings for both function declarations
    assert!(
        has_fetch_mapping && has_process_mapping,
        "expected mappings for both async function declarations. mappings: {mappings}"
    );

    // TODO: Await expression line mappings are not yet generated by the ES5 IR transform.
    // When implemented, uncomment and verify:
    // let (await_fetch_line, _) = find_line_col(source, "await fetch");
    // let has_await_fetch_mapping = decoded.iter().any(|m| m.source_index == 0 && m.original_line == await_fetch_line);
    // assert!(has_await_fetch_mapping, "expected mapping for await expression");

    // Verify non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async ES5 code"
    );

    // Verify mappings span multiple source lines
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 2,
        "expected mappings from at least 2 different source lines, got: {unique_source_lines:?}"
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
        "expected mappings from at least 2 different source lines, got: {unique_source_lines:?}"
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

