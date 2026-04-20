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

