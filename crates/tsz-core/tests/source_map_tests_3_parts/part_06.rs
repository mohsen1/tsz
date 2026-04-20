#[test]
fn test_source_map_let_const_declaration() {
    // Test let and const declarations (downleveled to var in ES5)
    let source = r#"let a = 1;
let b = 2;
const c = 3;
const d = a + b + c;
console.log(d);"#;

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
        output.contains("a") && output.contains("b") && output.contains("c"),
        "expected output to contain a, b, c. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for let/const declaration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_multiple_declarators() {
    // Test multiple variable declarators in single statement
    let source = r#"var a = 1, b = 2, c = 3;
let x = 10, y = 20, z = 30;
const m = 100, n = 200;
console.log(a, b, c, x, y, z, m, n);"#;

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
        output.contains("a") && output.contains("b") && output.contains("c"),
        "expected output to contain a, b, c. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple declarators"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_with_type() {
    // Test declarations with type annotations
    let source = r#"let num: number = 42;
let str: string = "hello";
let bool: boolean = true;
let arr: number[] = [1, 2, 3];
let obj: { x: number; y: number } = { x: 1, y: 2 };
let fn: (a: number) => number = (a) => a * 2;

console.log(num, str, bool, arr, obj, fn(5));"#;

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
        output.contains("num") && output.contains("str"),
        "expected output to contain num and str. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for typed declarations"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_destructuring_object() {
    // Test object destructuring declarations
    let source = r#"const obj = { a: 1, b: 2, c: 3 };
const { a, b } = obj;
const { c: renamed } = obj;
const { a: x, b: y, ...rest } = { a: 1, b: 2, c: 3, d: 4 };

console.log(a, b, renamed, x, y, rest);"#;

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
        output.contains("obj"),
        "expected output to contain obj. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_destructuring_array() {
    // Test array destructuring declarations
    let source = r#"const arr = [1, 2, 3, 4, 5];
const [first, second] = arr;
const [a, , c] = arr;
const [head, ...tail] = arr;
const [x, y, z = 10] = [1, 2];

console.log(first, second, a, c, head, tail, x, y, z);"#;

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
        output.contains("arr"),
        "expected output to contain arr. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array destructuring"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_in_function() {
    // Test variable declarations inside functions
    let source = r#"function processData(input: number): number {
    var multiplier = 2;
    let result = input * multiplier;
    const final = result + 10;

    if (final > 50) {
        let bonus = 5;
        return final + bonus;
    }

    return final;
}

const output = processData(25);"#;

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
        output.contains("processData"),
        "expected output to contain processData. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for declarations in function"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_in_for_loop() {
    // Test variable declarations in for loops
    let source = r#"const items = [1, 2, 3, 4, 5];
let sum = 0;

for (let i = 0; i < items.length; i++) {
    sum += items[i];
}

for (var j = 0; j < 3; j++) {
    console.log(j);
}

for (const item of items) {
    console.log(item);
}

for (const [index, value] of items.entries()) {
    console.log(index, value);
}

console.log(sum);"#;

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
        output.contains("items") || output.contains("sum"),
        "expected output to contain items or sum. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for declarations in for loop"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_complex_initializers() {
    // Test declarations with complex initializers
    let source = r#"const fn = function(x: number): number { return x * 2; };
const arrow = (y: number): number => y + 1;
const obj = { method(): number { return 42; } };
const arr = [1, 2, 3].map((n) => n * 2);
const cond = true ? "yes" : "no";
const template = `value is ${42}`;

const nested = {
    data: [1, 2, 3],
    process(): number[] {
        return this.data.map((n) => n * 2);
    }
};

console.log(fn(5), arrow(10), obj.method(), arr, cond, template);"#;

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
        output.contains("fn") || output.contains("arrow"),
        "expected output to contain fn or arrow. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for complex initializers"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_declaration_combined() {
    // Test combined variable declaration patterns
    let source = r#"class DataStore {
    private items: string[] = [];

    constructor() {
        const initial = ["a", "b", "c"];
        this.items = initial;
    }

    process(): { count: number; items: string[] } {
        var count = 0;
        let filtered: string[] = [];
        const threshold = 1;

        for (let i = 0; i < this.items.length; i++) {
            const item = this.items[i];
            if (item.length > threshold) {
                filtered.push(item);
                count++;
            }
        }

        const { length: total } = filtered;
        const [first = "none", ...rest] = filtered;

        let result = { count, items: filtered };
        return result;
    }
}

const store = new DataStore();
const { count, items } = store.process();
console.log(count, items);"#;

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
        output.contains("DataStore") || output.contains("process"),
        "expected output to contain DataStore or process. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined declaration patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Function Declaration ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_function_declaration_basic() {
    // Test basic function declaration
    let source = r#"function greet() {
    console.log("Hello");
}

function sayGoodbye() {
    console.log("Goodbye");
}

greet();
sayGoodbye();"#;

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
        output.contains("greet") && output.contains("sayGoodbye"),
        "expected output to contain greet and sayGoodbye. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function declaration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

