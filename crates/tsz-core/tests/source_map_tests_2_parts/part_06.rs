#[test]
fn test_source_map_for_await_combined() {
    // Test combined for-await-of patterns
    let source = r#"class EventStream {
    private events: Array<{ type: string; data: any }> = [];

    async *[Symbol.asyncIterator]() {
        for (const event of this.events) {
            yield event;
        }
    }

    push(type: string, data: any) {
        this.events.push({ type, data });
    }
}

async function processEvents(stream: EventStream) {
    const results: any[] = [];

    try {
        for await (const { type, data } of stream) {
            if (type === "error") {
                throw new Error(data);
            }
            if (type === "end") {
                break;
            }
            results.push(await Promise.resolve(data));
        }
    } catch (e) {
        console.error("Error:", e);
    }

    return results;
}

const stream = new EventStream();
stream.push("data", 1);
stream.push("data", 2);
stream.push("end", null);
processEvents(stream);"#;

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

    assert!(
        output.contains("EventStream") || output.contains("processEvents"),
        "expected output to contain class or function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined for-await-of patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Try-Catch-Finally ES5 Source Map Tests
// ============================================================================

#[test]
fn test_source_map_try_catch_basic() {
    // Test basic try-catch
    let source = r#"function riskyOperation() {
    try {
        const result = JSON.parse('invalid json');
        return result;
    } catch (error) {
        console.error("Parse error:", error);
        return null;
    }
}

riskyOperation();"#;

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

    assert!(
        output.contains("riskyOperation") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        output.contains("try") && output.contains("catch"),
        "expected output to contain try-catch. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic try-catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_finally() {
    // Test try-catch-finally
    let source = r#"function withCleanup() {
    const resource = { open: true };
    try {
        console.log("Using resource");
        throw new Error("Something went wrong");
    } catch (e) {
        console.error("Error:", e);
    } finally {
        resource.open = false;
        console.log("Resource closed");
    }
    return resource;
}

withCleanup();"#;

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

    assert!(
        output.contains("withCleanup") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        output.contains("finally"),
        "expected output to contain finally. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for try-catch-finally"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_finally() {
    // Test try-finally without catch
    let source = r#"function guaranteedCleanup() {
    let locked = true;
    try {
        console.log("Doing work while locked");
        return "done";
    } finally {
        locked = false;
        console.log("Lock released");
    }
}

guaranteedCleanup();"#;

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

    assert!(
        output.contains("guaranteedCleanup") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        output.contains("try") && output.contains("finally"),
        "expected output to contain try-finally. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for try-finally"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_nested() {
    // Test nested try-catch blocks
    let source = r#"function nestedTryCatch() {
    try {
        console.log("Outer try");
        try {
            console.log("Inner try");
            throw new Error("Inner error");
        } catch (innerError) {
            console.log("Inner catch:", innerError);
            throw new Error("Rethrown from inner");
        }
    } catch (outerError) {
        console.log("Outer catch:", outerError);
    }
}

nestedTryCatch();"#;

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

    assert!(
        output.contains("nestedTryCatch") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested try-catch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_typed() {
    // Test try-catch with typed catch clause
    let source = r#"class CustomError extends Error {
    constructor(public code: number, message: string) {
        super(message);
        this.name = "CustomError";
    }
}

function handleTypedError() {
    try {
        throw new CustomError(404, "Not found");
    } catch (e: unknown) {
        if (e instanceof CustomError) {
            console.log("Custom error code:", e.code);
        } else if (e instanceof Error) {
            console.log("Generic error:", e.message);
        }
    }
}

handleTypedError();"#;

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

    assert!(
        output.contains("handleTypedError") || output.contains("CustomError"),
        "expected output to contain function or class name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for try-catch with typed error"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_rethrow() {
    // Test try-catch with rethrow
    let source = r#"function validateAndProcess(data: string) {
    try {
        if (!data) {
            throw new Error("Data is required");
        }
        return JSON.parse(data);
    } catch (e) {
        console.error("Validation failed");
        throw e;
    }
}

try {
    validateAndProcess("");
} catch (e) {
    console.log("Caught rethrown error:", e);
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

    assert!(
        output.contains("validateAndProcess") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for try-catch with rethrow"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_async() {
    // Test try-catch in async function
    let source = r#"async function fetchWithRetry(url: string, retries: number = 3): Promise<string> {
    for (let i = 0; i < retries; i++) {
        try {
            const response = await fetch(url);
            if (!response.ok) {
                throw new Error("HTTP error");
            }
            return await response.text();
        } catch (e) {
            console.log(`Attempt ${i + 1} failed:`, e);
            if (i === retries - 1) {
                throw e;
            }
        }
    }
    throw new Error("Should not reach here");
}

fetchWithRetry("https://example.com");"#;

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

    assert!(
        output.contains("fetchWithRetry") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for try-catch in async function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_expression() {
    // Test try-catch with expression in throw
    let source = r#"function conditionalThrow(condition: boolean) {
    try {
        if (condition) {
            throw condition ? new Error("Condition true") : new Error("Condition false");
        }
        return "success";
    } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        return "error: " + message;
    }
}

console.log(conditionalThrow(true));
console.log(conditionalThrow(false));"#;

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

    assert!(
        output.contains("conditionalThrow") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for try-catch with throw expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_class_method() {
    // Test try-catch in class method
    let source = r#"class DatabaseConnection {
    private connected: boolean = false;

    connect(): void {
        try {
            this.connected = true;
            console.log("Connected");
        } catch (e) {
            this.connected = false;
            throw new Error("Connection failed: " + e);
        }
    }

    query(sql: string): any[] {
        try {
            if (!this.connected) {
                throw new Error("Not connected");
            }
            return [{ result: sql }];
        } catch (e) {
            console.error("Query error:", e);
            return [];
        } finally {
            console.log("Query completed");
        }
    }
}

const db = new DatabaseConnection();
db.connect();
db.query("SELECT * FROM users");"#;

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

    assert!(
        output.contains("DatabaseConnection") || output.contains("function"),
        "expected output to contain class name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for try-catch in class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_try_catch_combined() {
    // Test combined try-catch-finally patterns
    let source = r#"class ResourceManager {
    private resources: Map<string, { data: any; locked: boolean }> = new Map();

    acquire(id: string): any {
        try {
            const resource = this.resources.get(id);
            if (!resource) {
                throw new Error("Resource not found: " + id);
            }
            if (resource.locked) {
                throw new Error("Resource locked: " + id);
            }
            resource.locked = true;
            return resource.data;
        } catch (e) {
            console.error("Acquire failed:", e);
            throw e;
        }
    }

    async processWithResource(id: string, processor: (data: any) => Promise<void>): Promise<void> {
        let acquired = false;
        try {
            const data = this.acquire(id);
            acquired = true;
            try {
                await processor(data);
            } catch (processingError) {
                console.error("Processing error:", processingError);
                throw processingError;
            }
        } finally {
            if (acquired) {
                const resource = this.resources.get(id);
                if (resource) {
                    resource.locked = false;
                }
                console.log("Resource released:", id);
            }
        }
    }
}

const manager = new ResourceManager();
manager.processWithResource("test", async (data) => {
    console.log("Processing:", data);
});"#;

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

    assert!(
        output.contains("ResourceManager") || output.contains("processWithResource"),
        "expected output to contain class or method name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined try-catch-finally patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Switch-Case ES5 Source Map Tests
// ============================================================================

#[test]
fn test_source_map_switch_basic() {
    // Test basic switch with cases
    let source = r#"function getDay(n: number): string {
    switch (n) {
        case 0:
            return "Sunday";
        case 1:
            return "Monday";
        case 2:
            return "Tuesday";
        case 3:
            return "Wednesday";
        case 4:
            return "Thursday";
        case 5:
            return "Friday";
        case 6:
            return "Saturday";
    }
    return "Unknown";
}

console.log(getDay(3));"#;

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

    assert!(
        output.contains("getDay") || output.contains("switch"),
        "expected output to contain function name or switch. output: {output}"
    );
    assert!(
        output.contains("case"),
        "expected output to contain case. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_default() {
    // Test switch with default case
    let source = r#"function classify(value: number): string {
    switch (value) {
        case 1:
            return "one";
        case 2:
            return "two";
        case 3:
            return "three";
        default:
            return "other";
    }
}

console.log(classify(5));"#;

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

    assert!(
        output.contains("classify") || output.contains("switch"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        output.contains("default"),
        "expected output to contain default. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for switch with default"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_fallthrough() {
    // Test switch with fall-through cases
    let source = r#"function isWeekend(day: string): boolean {
    switch (day) {
        case "Saturday":
        case "Sunday":
            return true;
        case "Monday":
        case "Tuesday":
        case "Wednesday":
        case "Thursday":
        case "Friday":
            return false;
        default:
            throw new Error("Invalid day");
    }
}

console.log(isWeekend("Saturday"));"#;

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

    assert!(
        output.contains("isWeekend") || output.contains("switch"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for switch with fall-through"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_break() {
    // Test switch with break statements
    let source = r#"function process(action: string): void {
    let result = "";
    switch (action) {
        case "start":
            result = "Starting...";
            console.log(result);
            break;
        case "stop":
            result = "Stopping...";
            console.log(result);
            break;
        case "pause":
            result = "Pausing...";
            console.log(result);
            break;
        default:
            result = "Unknown action";
            console.log(result);
            break;
    }
}

process("start");"#;

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

    assert!(
        output.contains("process") || output.contains("switch"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        output.contains("break"),
        "expected output to contain break. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for switch with break"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_return() {
    // Test switch with return statements
    let source = r#"function getColor(code: number): string {
    switch (code) {
        case 0: return "black";
        case 1: return "red";
        case 2: return "green";
        case 3: return "yellow";
        case 4: return "blue";
        case 5: return "magenta";
        case 6: return "cyan";
        case 7: return "white";
        default: return "unknown";
    }
}

const colors = [0, 1, 2, 3].map(getColor);
console.log(colors);"#;

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

    assert!(
        output.contains("getColor") || output.contains("switch"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for switch with return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_nested() {
    // Test nested switch statements
    let source = r#"function classify(category: string, subcategory: number): string {
    switch (category) {
        case "animal":
            switch (subcategory) {
                case 1: return "dog";
                case 2: return "cat";
                default: return "unknown animal";
            }
        case "plant":
            switch (subcategory) {
                case 1: return "tree";
                case 2: return "flower";
                default: return "unknown plant";
            }
        default:
            return "unknown category";
    }
}

console.log(classify("animal", 1));"#;

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

    assert!(
        output.contains("classify") || output.contains("switch"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_in_function() {
    // Test switch inside various function types
    let source = r#"const handler = (event: string) => {
    switch (event) {
        case "click":
            return "clicked";
        case "hover":
            return "hovered";
        default:
            return "unknown event";
    }
};

const asyncHandler = async (event: string): Promise<string> => {
    switch (event) {
        case "load":
            return await Promise.resolve("loaded");
        case "error":
            return await Promise.resolve("errored");
        default:
            return "unknown";
    }
};

console.log(handler("click"));
asyncHandler("load").then(console.log);"#;

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

    assert!(
        output.contains("handler") || output.contains("switch"),
        "expected output to contain variable name or switch. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for switch in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_expression_cases() {
    // Test switch with expression cases
    let source = r#"const MODE_READ = 1;
const MODE_WRITE = 2;
const MODE_EXECUTE = 4;

function checkPermission(mode: number): string {
    switch (mode) {
        case MODE_READ:
            return "read";
        case MODE_WRITE:
            return "write";
        case MODE_EXECUTE:
            return "execute";
        case MODE_READ | MODE_WRITE:
            return "read-write";
        case MODE_READ | MODE_EXECUTE:
            return "read-execute";
        default:
            return "unknown";
    }
}

console.log(checkPermission(MODE_READ | MODE_WRITE));"#;

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

    assert!(
        output.contains("checkPermission") || output.contains("switch"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for switch with expression cases"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_class_method() {
    // Test switch in class method
    let source = r#"class StateMachine {
    private state: string = "idle";

    transition(action: string): void {
        switch (this.state) {
            case "idle":
                if (action === "start") {
                    this.state = "running";
                }
                break;
            case "running":
                switch (action) {
                    case "pause":
                        this.state = "paused";
                        break;
                    case "stop":
                        this.state = "stopped";
                        break;
                }
                break;
            case "paused":
                if (action === "resume") {
                    this.state = "running";
                }
                break;
            default:
                console.log("Unknown state");
        }
    }

    getState(): string {
        return this.state;
    }
}

const machine = new StateMachine();
machine.transition("start");
console.log(machine.getState());"#;

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

    assert!(
        output.contains("StateMachine") || output.contains("transition"),
        "expected output to contain class or method name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for switch in class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_switch_combined() {
    // Test combined switch patterns
    let source = r#"enum HttpStatus {
    OK = 200,
    Created = 201,
    BadRequest = 400,
    NotFound = 404,
    InternalError = 500
}

interface Response {
    status: HttpStatus;
    message: string;
}

class HttpHandler {
    handleResponse(response: Response): string {
        switch (response.status) {
            case HttpStatus.OK:
            case HttpStatus.Created:
                return this.handleSuccess(response);
            case HttpStatus.BadRequest:
                return this.handleClientError(response);
            case HttpStatus.NotFound:
                return this.handleNotFound();
            case HttpStatus.InternalError:
                return this.handleServerError();
            default:
                return this.handleUnknown(response.status);
        }
    }

    private handleSuccess(response: Response): string {
        switch (response.status) {
            case HttpStatus.OK:
                return "OK: " + response.message;
            case HttpStatus.Created:
                return "Created: " + response.message;
            default:
                return "Success";
        }
    }

    private handleClientError(response: Response): string {
        return "Client Error: " + response.message;
    }

    private handleNotFound(): string {
        return "Not Found";
    }

    private handleServerError(): string {
        return "Internal Server Error";
    }

    private handleUnknown(status: HttpStatus): string {
        return "Unknown status: " + status;
    }
}

const handler = new HttpHandler();
console.log(handler.handleResponse({ status: HttpStatus.OK, message: "Success" }));"#;

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

    assert!(
        output.contains("HttpHandler") || output.contains("handleResponse"),
        "expected output to contain class or method name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined switch patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// Labeled Statement ES5 Source Map Tests
// ============================================================================

#[test]
fn test_source_map_labeled_basic() {
    // Test basic labeled statement with break
    let source = r#"function findValue(matrix: number[][], target: number): boolean {
    outer: {
        for (let i = 0; i < matrix.length; i++) {
            for (let j = 0; j < matrix[i].length; j++) {
                if (matrix[i][j] === target) {
                    console.log("Found at", i, j);
                    break outer;
                }
            }
        }
        console.log("Not found");
    }
    return true;
}

findValue([[1, 2], [3, 4]], 3);"#;

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

    assert!(
        output.contains("findValue") || output.contains("outer"),
        "expected output to contain function name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic labeled statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_for_break() {
    // Test labeled for loop with break
    let source = r#"function searchGrid(grid: string[][]): { row: number; col: number } | null {
    let result: { row: number; col: number } | null = null;

    search: for (let row = 0; row < grid.length; row++) {
        for (let col = 0; col < grid[row].length; col++) {
            if (grid[row][col] === "X") {
                result = { row, col };
                break search;
            }
        }
    }

    return result;
}

const grid = [[".", "."], [".", "X"]];
console.log(searchGrid(grid));"#;

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

    assert!(
        output.contains("searchGrid") || output.contains("search"),
        "expected output to contain function name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled for loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_while_continue() {
    // Test labeled while loop with continue
    let source = r#"function processItems(items: number[][]): number {
    let total = 0;
    let i = 0;

    outer: while (i < items.length) {
        let j = 0;
        while (j < items[i].length) {
            if (items[i][j] < 0) {
                i++;
                continue outer;
            }
            total += items[i][j];
            j++;
        }
        i++;
    }

    return total;
}

console.log(processItems([[1, 2], [-1, 3], [4, 5]]));"#;

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

    assert!(
        output.contains("processItems") || output.contains("outer"),
        "expected output to contain function name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled while loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_nested() {
    // Test nested labeled loops
    let source = r#"function findPath(maze: number[][]): string[] {
    const path: string[] = [];

    level1: for (let i = 0; i < maze.length; i++) {
        level2: for (let j = 0; j < maze[i].length; j++) {
            level3: for (let k = 0; k < 3; k++) {
                if (maze[i][j] === k) {
                    path.push(`${i},${j},${k}`);
                    if (k === 2) break level1;
                    if (k === 1) break level2;
                    continue level3;
                }
            }
        }
    }

    return path;
}

console.log(findPath([[0, 1], [2, 0]]));"#;

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

    assert!(
        output.contains("findPath") || output.contains("level"),
        "expected output to contain function name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested labeled loops"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_labeled_block() {
    // Test labeled block statement
    let source = r#"function processData(data: any): string {
    let result = "";

    validation: {
        if (!data) {
            result = "No data";
            break validation;
        }
        if (!data.name) {
            result = "No name";
            break validation;
        }
        if (!data.value) {
            result = "No value";
            break validation;
        }
        result = "Valid: " + data.name + " = " + data.value;
    }

    return result;
}

console.log(processData({ name: "test", value: 42 }));
console.log(processData(null));"#;

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

    assert!(
        output.contains("processData") || output.contains("validation"),
        "expected output to contain function name or label. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled block"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

