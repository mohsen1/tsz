#[test]
fn test_source_map_break_basic() {
    // Test basic break statement in loop
    let source = r#"function findFirst(arr: number[], target: number): number {
    for (let i = 0; i < arr.length; i++) {
        if (arr[i] === target) {
            break;
        }
    }
    return -1;
}

const result = findFirst([1, 2, 3, 4, 5], 3);"#;

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
        output.contains("findFirst"),
        "expected output to contain findFirst. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_continue_basic() {
    // Test basic continue statement in loop
    let source = r#"function sumPositive(arr: number[]): number {
    let sum = 0;
    for (let i = 0; i < arr.length; i++) {
        if (arr[i] < 0) {
            continue;
        }
        sum += arr[i];
    }
    return sum;
}

const result = sumPositive([1, -2, 3, -4, 5]);"#;

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
        output.contains("sumPositive"),
        "expected output to contain sumPositive. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for continue statement"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_while() {
    // Test break in while loop
    let source = r#"function readUntilEnd(data: string[]): string[] {
    const results: string[] = [];
    let i = 0;
    while (i < data.length) {
        if (data[i] === "END") {
            break;
        }
        results.push(data[i]);
        i++;
    }
    return results;
}

const output = readUntilEnd(["a", "b", "END", "c"]);"#;

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
        output.contains("readUntilEnd"),
        "expected output to contain readUntilEnd. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break in while"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_continue_while() {
    // Test continue in while loop
    let source = r#"function processNonEmpty(items: string[]): string[] {
    const results: string[] = [];
    let i = 0;
    while (i < items.length) {
        const item = items[i];
        i++;
        if (item === "") {
            continue;
        }
        results.push(item.toUpperCase());
    }
    return results;
}

const processed = processNonEmpty(["a", "", "b", "", "c"]);"#;

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
        output.contains("processNonEmpty"),
        "expected output to contain processNonEmpty. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for continue in while"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_labeled() {
    // Test labeled break statement
    let source = r#"function findInMatrix(matrix: number[][], target: number): [number, number] | null {
    outer: for (let i = 0; i < matrix.length; i++) {
        for (let j = 0; j < matrix[i].length; j++) {
            if (matrix[i][j] === target) {
                break outer;
            }
        }
    }
    return null;
}

const pos = findInMatrix([[1, 2], [3, 4]], 3);"#;

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
        output.contains("findInMatrix"),
        "expected output to contain findInMatrix. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled break"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_continue_labeled() {
    // Test labeled continue statement
    let source = r#"function processRows(matrix: number[][]): number[] {
    const results: number[] = [];
    outer: for (let i = 0; i < matrix.length; i++) {
        for (let j = 0; j < matrix[i].length; j++) {
            if (matrix[i][j] < 0) {
                continue outer;
            }
        }
        results.push(i);
    }
    return results;
}

const validRows = processRows([[1, 2], [-1, 2], [3, 4]]);"#;

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
        output.contains("processRows"),
        "expected output to contain processRows. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for labeled continue"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_switch() {
    // Test break in switch statement
    let source = r#"function getDayName(day: number): string {
    let name: string;
    switch (day) {
        case 0:
            name = "Sunday";
            break;
        case 1:
            name = "Monday";
            break;
        case 2:
            name = "Tuesday";
            break;
        default:
            name = "Unknown";
            break;
    }
    return name;
}

const today = getDayName(1);"#;

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
        output.contains("getDayName") || output.contains("switch"),
        "expected output to contain getDayName or switch. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break in switch"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_do_while() {
    // Test break in do-while loop
    let source = r#"function readInput(): number {
    let value = 0;
    let attempts = 0;
    do {
        attempts++;
        value = Math.random();
        if (value > 0.5) {
            break;
        }
    } while (attempts < 10);
    return value;
}

const input = readInput();"#;

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
        output.contains("readInput"),
        "expected output to contain readInput. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for break in do-while"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_break_continue_combined() {
    // Test combined break and continue patterns
    let source = r#"class DataIterator {
    private data: number[][] = [];

    constructor(data: number[][]) {
        this.data = data;
    }

    processAll(): number[] {
        const results: number[] = [];

        outer: for (let i = 0; i < this.data.length; i++) {
            const row = this.data[i];

            if (row.length === 0) {
                continue;
            }

            for (let j = 0; j < row.length; j++) {
                if (row[j] < 0) {
                    continue outer;
                }
                if (row[j] > 100) {
                    break outer;
                }
                results.push(row[j]);
            }
        }

        return results;
    }

    findValue(target: number): boolean {
        let found = false;
        let i = 0;

        while (i < this.data.length) {
            let j = 0;
            while (j < this.data[i].length) {
                if (this.data[i][j] === target) {
                    found = true;
                    break;
                }
                j++;
            }
            if (found) {
                break;
            }
            i++;
        }

        return found;
    }

    sumPositiveByRow(): number[] {
        const sums: number[] = [];

        for (let i = 0; i < this.data.length; i++) {
            let sum = 0;
            for (const val of this.data[i]) {
                if (val < 0) {
                    continue;
                }
                sum += val;
            }
            sums.push(sum);
        }

        return sums;
    }
}

const iterator = new DataIterator([[1, 2], [3, -4], [5, 6]]);
const processed = iterator.processAll();
const found = iterator.findValue(3);
const sums = iterator.sumPositiveByRow();"#;

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
        output.contains("DataIterator") || output.contains("processAll"),
        "expected output to contain DataIterator or processAll. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined break/continue patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Expression Statement ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_expression_function_call() {
    // Test function call expression statement
    let source = r#"function greet(name: string): void {
    console.log("Hello, " + name);
}

greet("World");
console.log("Done");
Math.random();"#;

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
        output.contains("greet"),
        "expected output to contain greet. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function call expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

