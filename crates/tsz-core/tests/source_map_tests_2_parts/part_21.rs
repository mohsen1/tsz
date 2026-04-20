#[test]
fn test_source_map_labeled_switch() {
    // Test labeled statement with switch
    let source = r#"function handleEvent(events: Array<{ type: string; data: any }>): void {
    eventLoop: for (const event of events) {
        switch (event.type) {
            case "skip":
                continue eventLoop;
            case "stop":
                console.log("Stopping");
                break eventLoop;
            case "process":
                console.log("Processing:", event.data);
                break;
            default:
                console.log("Unknown event:", event.type);
        }
    }
    console.log("Event loop finished");
}

handleEvent([
    { type: "process", data: 1 },
    { type: "skip", data: 2 },
    { type: "stop", data: 3 }
]);"#;

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

    assert!(
        output.contains("handleEvent") || output.contains("eventLoop"),
        "expected output to contain function name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_in_function() {
    // Test labeled statement in different function types
    let source = r#"const processor = (items: number[]) => {
    mainLoop: for (let i = 0; i < items.length; i++) {
        if (items[i] < 0) break mainLoop;
        console.log(items[i]);
    }
};

async function asyncProcessor(items: Promise<number>[]): Promise<void> {
    asyncLoop: for (let i = 0; i < items.length; i++) {
        const value = await items[i];
        if (value < 0) break asyncLoop;
        console.log(value);
    }
}

processor([1, 2, -1, 3]);
asyncProcessor([Promise.resolve(1), Promise.resolve(-1)]);"#;

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

    assert!(
        output.contains("processor") || output.contains("mainLoop"),
        "expected output to contain variable name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_do_while() {
    // Test labeled do-while loop
    let source = r#"function retryOperation(maxRetries: number): boolean {
    let attempts = 0;
    let success = false;

    retryLoop: do {
        attempts++;
        console.log("Attempt", attempts);

        if (Math.random() > 0.7) {
            success = true;
            break retryLoop;
        }

        if (attempts >= maxRetries) {
            console.log("Max retries reached");
            break retryLoop;
        }
    } while (true);

    return success;
}

retryOperation(5);"#;

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

    assert!(
        output.contains("retryOperation") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled do-while"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_class_method() {
    // Test labeled statement in class method
    let source = r#"class DataProcessor {
    private data: number[][] = [];

    process(): number[] {
        const results: number[] = [];

        rowLoop: for (let i = 0; i < this.data.length; i++) {
            colLoop: for (let j = 0; j < this.data[i].length; j++) {
                const value = this.data[i][j];

                if (value < 0) {
                    continue rowLoop;
                }

                if (value > 100) {
                    break colLoop;
                }

                results.push(value);
            }
        }

        return results;
    }

    setData(data: number[][]): void {
        this.data = data;
    }
}

const processor = new DataProcessor();
processor.setData([[1, 2, -3], [4, 150, 6]]);
console.log(processor.process());"#;

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

    assert!(
        output.contains("DataProcessor") || output.contains("rowLoop"),
        "expected output to contain class name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled in class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_combined() {
    // Test combined labeled statement patterns
    let source = r#"interface Task {
    id: number;
    subtasks: Task[];
    status: "pending" | "done" | "skipped";
}

class TaskRunner {
    run(tasks: Task[]): number[] {
        const completed: number[] = [];

        taskLoop: for (const task of tasks) {
            validation: {
                if (task.status === "skipped") {
                    continue taskLoop;
                }
                if (task.status === "done") {
                    completed.push(task.id);
                    break validation;
                }

                subtaskLoop: for (const subtask of task.subtasks) {
                    switch (subtask.status) {
                        case "skipped":
                            continue subtaskLoop;
                        case "pending":
                            console.log("Pending subtask:", subtask.id);
                            break taskLoop;
                        case "done":
                            completed.push(subtask.id);
                            break;
                    }
                }
            }
        }

        return completed;
    }
}

const runner = new TaskRunner();
const tasks: Task[] = [
    { id: 1, subtasks: [{ id: 11, subtasks: [], status: "done" }], status: "pending" },
    { id: 2, subtasks: [], status: "done" }
];
console.log(runner.run(tasks));"#;

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

    assert!(
        output.contains("TaskRunner") || output.contains("taskLoop"),
        "expected output to contain class name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined labeled patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// With Statement ES5 Source Map Tests
// ============================================================================

#[test]
fn test_source_map_with_basic() {
    // Test basic with statement
    let source = r#"var obj = { x: 10, y: 20 };

with (obj) {
    console.log(x);
    console.log(y);
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
        output.contains("with") || output.contains("obj"),
        "expected output to contain with or obj. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic with statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_property_access() {
    // Test with statement with property access
    let source = r#"var config = {
    settings: {
        debug: true,
        verbose: false
    }
};

with (config.settings) {
    if (debug) {
        console.log("Debug mode");
    }
    if (verbose) {
        console.log("Verbose mode");
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
        output.contains("config") || output.contains("with"),
        "expected output to contain config or with. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for with property access"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_method_call() {
    // Test with statement with method calls
    let source = r#"var math = {
    PI: 3.14159,
    square: function(x) { return x * x; },
    cube: function(x) { return x * x * x; }
};

with (math) {
    var area = PI * square(5);
    var volume = PI * cube(3);
    console.log(area, volume);
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
        output.contains("math") || output.contains("with"),
        "expected output to contain math or with. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for with method call"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_nested() {
    // Test nested with statements
    let source = r#"var outer = {
    name: "outer",
    inner: {
        name: "inner",
        value: 42
    }
};

with (outer) {
    console.log(name);
    with (inner) {
        console.log(name, value);
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
        output.contains("outer") || output.contains("with"),
        "expected output to contain outer or with. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested with statements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_with_function() {
    // Test with statement with function definition
    let source = r#"var context = {
    multiplier: 10,
    offset: 5
};

function calculate(value) {
    with (context) {
        return value * multiplier + offset;
    }
}

console.log(calculate(3));"#;

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
        output.contains("calculate") || output.contains("function"),
        "expected output to contain calculate or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for with function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

