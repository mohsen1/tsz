#[test]
fn test_source_map_es5_transform_async_for_loop_header_awaits_mapping() {
    let source = "async function run() {\n    for (let i = await init(); await cond(i); i = await step(i)) {\n        await body(i);\n    }\n}";
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

