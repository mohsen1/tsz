#[test]
fn test_source_map_es5_transform_async_template_literal_mapping() {
    let source = "async function run(){ return `value ${await foo()}`; }";
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
fn test_source_map_es5_transform_async_try_finally_await_in_finally_direct_mapping() {
    let source = "async function run() {\n    try {\n        await work();\n    } finally {\n        const done = await cleanup();\n        report(done);\n    }\n}";
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
fn test_source_map_es5_transform_async_try_catch_finally_mapping() {
    let source =
        "async function run() { try { await foo(); } catch { bar(); } finally { baz(); } }";
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

