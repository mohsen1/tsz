#[test]
fn test_source_map_for_await_class_method() {
    // Test for-await-of in class method
    let source = r#"class AsyncProcessor {
    private results: number[] = [];

    async process(source: AsyncIterable<number>): Promise<number[]> {
        for await (const item of source) {
            this.results.push(item * 2);
        }
        return this.results;
    }

    async *generate(): AsyncGenerator<number> {
        yield 1;
        yield 2;
        yield 3;
    }
}

const processor = new AsyncProcessor();
processor.process(processor.generate());"#;

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
        output.contains("AsyncProcessor") || output.contains("function"),
        "expected output to contain class name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-await-of in class method"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_for_await_with_await() {
    // Test for-await-of with additional await expressions
    let source = r#"async function* dataGen() {
    yield { id: 1, url: "url1" };
    yield { id: 2, url: "url2" };
}

async function fetchData(url: string): Promise<string> {
    return "data from " + url;
}

async function processWithFetch() {
    for await (const { id, url } of dataGen()) {
        const data = await fetchData(url);
        console.log(id, data);
    }
}

processWithFetch();"#;

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
        output.contains("processWithFetch") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-await-of with await"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_for_await_label() {
    // Test for-await-of with labels
    let source = r#"async function* gen1() { yield 1; yield 2; }
async function* gen2() { yield "a"; yield "b"; }

async function labeledLoop() {
    outer: for await (const num of gen1()) {
        inner: for await (const str of gen2()) {
            if (num === 2 && str === "a") {
                break outer;
            }
            console.log(num, str);
        }
    }
}

labeledLoop();"#;

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
        output.contains("labeledLoop") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-await-of with labels"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_for_await_return() {
    // Test for-await-of with return statement
    let source = r#"async function* searchStream() {
    yield { id: 1, found: false };
    yield { id: 2, found: true };
    yield { id: 3, found: false };
}

async function findFirst(): Promise<number | null> {
    for await (const { id, found } of searchStream()) {
        if (found) {
            return id;
        }
    }
    return null;
}

findFirst().then(console.log);"#;

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
        output.contains("findFirst") || output.contains("function"),
        "expected output to contain function name. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-await-of with return"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

