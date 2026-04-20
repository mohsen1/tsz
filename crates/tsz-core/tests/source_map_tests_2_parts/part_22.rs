#[test]
fn test_source_map_with_loop() {
    // Test with statement with loop
    let source = r#"var data = {
    items: [1, 2, 3, 4, 5],
    sum: 0
};

with (data) {
    for (var i = 0; i < items.length; i++) {
        sum += items[i];
    }
    console.log("Sum:", sum);
}"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
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
    printer.enable_source_map("test.js", "test.js");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("data") || output.contains("for"),
        "expected output to contain data or for. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for with loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_conditional() {
    // Test with statement with conditionals
    let source = r#"var user = {
    name: "John",
    age: 25,
    isAdmin: false
};

with (user) {
    if (isAdmin) {
        console.log(name + " is an admin");
    } else if (age >= 18) {
        console.log(name + " is an adult");
    } else {
        console.log(name + " is a minor");
    }
}"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
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
    printer.enable_source_map("test.js", "test.js");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("user") || output.contains("if"),
        "expected output to contain user or if. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for with conditional"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_assignment() {
    // Test with statement with assignments
    let source = r#"var state = {
    count: 0,
    message: ""
};

with (state) {
    count = 10;
    message = "Updated";
    count++;
    message += "!";
}

console.log(state.count, state.message);"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
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
    printer.enable_source_map("test.js", "test.js");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("state") || output.contains("with"),
        "expected output to contain state or with. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for with assignment"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_try_catch() {
    // Test with statement with try-catch
    let source = r#"var errorHandler = {
    log: function(msg) { console.log("Error:", msg); },
    count: 0
};

with (errorHandler) {
    try {
        throw new Error("Test error");
    } catch (e) {
        log(e.message);
        count++;
    }
}

console.log("Error count:", errorHandler.count);"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
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
    printer.enable_source_map("test.js", "test.js");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("errorHandler") || output.contains("try"),
        "expected output to contain errorHandler or try. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for with try-catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_combined() {
    // Test combined with statement patterns
    let source = r#"var app = {
    config: {
        debug: true,
        version: "1.0"
    },
    utils: {
        format: function(s) { return "[" + s + "]"; },
        log: function(msg) { console.log(msg); }
    },
    data: {
        items: ["a", "b", "c"],
        count: 0
    }
};

function processApp() {
    with (app) {
        with (config) {
            if (debug) {
                with (utils) {
                    log(format("Version: " + version));
                }
            }
        }

        with (data) {
            for (var i = 0; i < items.length; i++) {
                with (app.utils) {
                    log(format(items[i]));
                }
                count++;
            }
        }
    }
    return app.data.count;
}

console.log("Processed:", processApp());"#;

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
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
    printer.enable_source_map("test.js", "test.js");
    printer.emit(root);

    let output = printer.get_output().to_string();
    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");

    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    assert!(
        output.contains("processApp") || output.contains("app"),
        "expected output to contain processApp or app. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined with patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Throw Statement ES5 Source Map Tests
// ============================================================================
