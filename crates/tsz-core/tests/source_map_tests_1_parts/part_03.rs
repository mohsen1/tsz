#[test]
fn test_source_map_es5_transform_async_try_catch_await_mapping() {
    let source = "async function run() { try { await foo(); } catch (err) { await bar(err); } }";
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
fn test_source_map_es5_transform_async_try_catch_throw_await_mapping() {
    let source =
        "async function run() { try { await foo(); } catch (err) { throw await bar(err); } }";
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
fn test_source_map_es5_transform_async_if_await_condition_mapping() {
    let source =
        "async function run(flag){ if (await foo(flag)) { bar(); } else { await baz(); } }";
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

