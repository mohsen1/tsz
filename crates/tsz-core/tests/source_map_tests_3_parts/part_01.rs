#[test]
fn test_source_map_debugger_basic() {
    // Test basic debugger statement
    let source = r#"function processData(data: any): void {
    debugger;
    console.log("Processing:", data);
}

processData({ value: 42 });"#;

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
        output.contains("debugger") || output.contains("processData"),
        "expected output to contain debugger or function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic debugger"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_in_function() {
    // Test debugger in function
    let source = r#"function calculate(a: number, b: number): number {
    debugger;
    const sum = a + b;
    debugger;
    return sum;
}

function main(): void {
    debugger;
    const result = calculate(10, 20);
    console.log("Result:", result);
}

main();"#;

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
        output.contains("calculate") || output.contains("main"),
        "expected output to contain function names. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_conditional() {
    // Test debugger in conditional
    let source = r#"function checkValue(value: number): string {
    if (value < 0) {
        debugger;
        return "negative";
    } else if (value === 0) {
        debugger;
        return "zero";
    } else {
        debugger;
        return "positive";
    }
}

console.log(checkValue(-5));
console.log(checkValue(0));
console.log(checkValue(10));"#;

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
        output.contains("checkValue") || output.contains("if"),
        "expected output to contain function name or if. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger conditional"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_loop() {
    // Test debugger in loop
    let source = r#"function processArray(arr: number[]): number {
    let sum = 0;
    for (let i = 0; i < arr.length; i++) {
        debugger;
        sum += arr[i];
    }
    return sum;
}

function processWhile(n: number): number {
    let count = 0;
    while (count < n) {
        debugger;
        count++;
    }
    return count;
}

console.log(processArray([1, 2, 3]));
console.log(processWhile(5));"#;

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
        output.contains("processArray") || output.contains("for"),
        "expected output to contain function name or for. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_class_method() {
    // Test debugger in class method
    let source = r#"class Calculator {
    private value: number = 0;

    add(n: number): this {
        debugger;
        this.value += n;
        return this;
    }

    subtract(n: number): this {
        debugger;
        this.value -= n;
        return this;
    }

    getValue(): number {
        debugger;
        return this.value;
    }
}

const calc = new Calculator();
console.log(calc.add(10).subtract(3).getValue());"#;

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
        output.contains("Calculator") || output.contains("function"),
        "expected output to contain Calculator or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_try_catch() {
    // Test debugger in try-catch
    let source = r#"function riskyOperation(value: number): number {
    try {
        debugger;
        if (value < 0) {
            throw new Error("Negative value");
        }
        return value * 2;
    } catch (e) {
        debugger;
        console.error("Error:", e);
        return 0;
    } finally {
        debugger;
        console.log("Cleanup");
    }
}

console.log(riskyOperation(5));
console.log(riskyOperation(-1));"#;

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
        output.contains("riskyOperation") || output.contains("try"),
        "expected output to contain function name or try. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger try-catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_arrow_function() {
    // Test debugger in arrow function
    let source = r#"const multiply = (a: number, b: number) => {
    debugger;
    return a * b;
};

const process = (arr: number[]) => {
    debugger;
    return arr.map((x) => {
        debugger;
        return x * 2;
    });
};

console.log(multiply(3, 4));
console.log(process([1, 2, 3]));"#;

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
        output.contains("multiply") || output.contains("function"),
        "expected output to contain multiply or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger arrow function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_async() {
    // Test debugger in async function
    let source = r#"async function fetchData(url: string): Promise<string> {
    debugger;
    const response = await fetch(url);
    debugger;
    return await response.text();
}

async function processUrls(urls: string[]): Promise<void> {
    for (const url of urls) {
        debugger;
        const data = await fetchData(url);
        console.log(data);
    }
}

processUrls(["https://example.com"]);"#;

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
        output.contains("fetchData") || output.contains("function"),
        "expected output to contain fetchData or function. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger async"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_switch() {
    // Test debugger in switch statement
    let source = r#"function handleAction(action: string): void {
    switch (action) {
        case "start":
            debugger;
            console.log("Starting...");
            break;
        case "stop":
            debugger;
            console.log("Stopping...");
            break;
        case "pause":
            debugger;
            console.log("Pausing...");
            break;
        default:
            debugger;
            console.log("Unknown action");
    }
}

handleAction("start");
handleAction("unknown");"#;

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
        output.contains("handleAction") || output.contains("switch"),
        "expected output to contain handleAction or switch. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for debugger switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_debugger_combined() {
    // Test combined debugger patterns
    let source = r#"class DataProcessor {
    private data: number[] = [];

    constructor(initialData: number[]) {
        debugger;
        this.data = initialData;
    }

    async process(): Promise<number[]> {
        debugger;
        const results: number[] = [];

        for (let i = 0; i < this.data.length; i++) {
            debugger;
            try {
                const value = this.data[i];
                if (value < 0) {
                    debugger;
                    throw new Error("Negative value");
                }
                results.push(value * 2);
            } catch (e) {
                debugger;
                results.push(0);
            }
        }

        return results;
    }

    filter(predicate: (n: number) => boolean): number[] {
        debugger;
        return this.data.filter((n) => {
            debugger;
            return predicate(n);
        });
    }
}

async function main(): Promise<void> {
    debugger;
    const processor = new DataProcessor([1, -2, 3, 4]);
    const processed = await processor.process();
    console.log("Processed:", processed);

    const filtered = processor.filter((n) => n > 0);
    console.log("Filtered:", filtered);
}

main();"#;

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
        output.contains("DataProcessor") || output.contains("process"),
        "expected output to contain DataProcessor or process. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined debugger patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Empty Statement ES5 Source Map Tests
// =============================================================================

