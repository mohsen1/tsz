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

