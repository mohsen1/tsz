#[test]
fn test_source_map_expression_new() {
    // Test new expression statements
    let source = r#"class Widget {
    constructor(public name: string) {}
}

new Widget("button");
new Date();
new Array(10);
new Map();
new Set([1, 2, 3]);

const widgets: Widget[] = [];
widgets.push(new Widget("checkbox"));"#;

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
        output.contains("Widget"),
        "expected output to contain Widget. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for new expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_delete_void_typeof() {
    // Test delete, void, typeof expression statements
    let source = r#"const obj: { [key: string]: number } = { a: 1, b: 2 };

delete obj.a;
delete obj["b"];

void 0;
void console.log("side effect");

typeof obj;
typeof undefined;"#;

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
        output.contains("obj") || output.contains("delete"),
        "expected output to contain obj or delete. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for delete/void/typeof"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_expression_combined() {
    // Test combined expression statement patterns
    let source = r#"class Counter {
    private count = 0;
    private history: number[] = [];

    increment(): void {
        this.count++;
        this.history.push(this.count);
    }

    decrement(): void {
        --this.count;
        this.history.push(this.count);
    }

    reset(): void {
        this.count = 0;
        this.history.length = 0;
    }

    log(): void {
        console.log("Count:", this.count);
        this.history.forEach((v, i) => console.log(i, v));
    }
}

const counter = new Counter();
counter.increment();
counter.increment();
counter.decrement();
counter.log();

let x = 0;
let y = 0;
x = y = 10;
x += 5;
y *= 2;

x > y ? console.log("x wins") : console.log("y wins");
x && y && console.log("both truthy");

const arr = [1, 2, 3];
arr.push(4);
arr.pop();
arr.sort((a, b) => a - b);
arr.reverse();"#;

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
        output.contains("Counter") || output.contains("increment"),
        "expected output to contain Counter or increment. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined expression patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Variable Declaration ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_var_declaration_basic() {
    // Test basic var declarations
    let source = r#"var x = 10;
var y = 20;
var z;
z = x + y;
console.log(z);"#;

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
        output.contains("var") && output.contains("x") && output.contains("y"),
        "expected output to contain var, x, y. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for var declaration"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_let_const_declaration() {
    // Test let and const declarations (downleveled to var in ES5)
    let source = r#"let a = 1;
let b = 2;
const c = 3;
const d = a + b + c;
console.log(d);"#;

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

#[test]
fn test_source_map_function_with_parameters() {
    // Test function with various parameter types
    let source = r#"function add(a: number, b: number): number {
    return a + b;
}

function greetPerson(name: string, age: number): string {
    return "Hello " + name + ", you are " + age;
}

function processArray(items: number[]): number {
    let sum = 0;
    for (const item of items) {
        sum += item;
    }
    return sum;
}

console.log(add(1, 2));
console.log(greetPerson("Alice", 30));
console.log(processArray([1, 2, 3]));"#;

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
        output.contains("add") && output.contains("greetPerson"),
        "expected output to contain add and greetPerson. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function with parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_default_parameters() {
    // Test function with default parameters
    let source = r#"function greet(name: string = "World"): string {
    return "Hello, " + name;
}

function createPoint(x: number = 0, y: number = 0): { x: number; y: number } {
    return { x, y };
}

function formatMessage(msg: string, prefix: string = "[INFO]", suffix: string = ""): string {
    return prefix + " " + msg + suffix;
}

console.log(greet());
console.log(greet("Alice"));
console.log(createPoint());
console.log(createPoint(10, 20));
console.log(formatMessage("Hello"));"#;

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
        output.contains("greet") || output.contains("createPoint"),
        "expected output to contain greet or createPoint. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_rest_parameters() {
    // Test function with rest parameters
    let source = r#"function sum(...numbers: number[]): number {
    return numbers.reduce((a, b) => a + b, 0);
}

function concat(separator: string, ...items: string[]): string {
    return items.join(separator);
}

function logAll(prefix: string, ...values: any[]): void {
    for (const value of values) {
        console.log(prefix, value);
    }
}

console.log(sum(1, 2, 3, 4, 5));
console.log(concat(", ", "a", "b", "c"));
logAll("[DEBUG]", "one", "two", "three");"#;

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
        output.contains("sum") || output.contains("concat"),
        "expected output to contain sum or concat. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest parameters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_nested() {
    // Test nested function declarations
    let source = r#"function outer(x: number): number {
    function inner(y: number): number {
        return y * 2;
    }

    function helper(z: number): number {
        return z + 1;
    }

    return inner(helper(x));
}

function createCounter(): () => number {
    let count = 0;

    function increment(): number {
        count++;
        return count;
    }

    return increment;
}

console.log(outer(5));
const counter = createCounter();
console.log(counter());
console.log(counter());"#;

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
        output.contains("outer") || output.contains("createCounter"),
        "expected output to contain outer or createCounter. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_generator() {
    // Test generator function declarations
    let source = r#"function* numberGenerator(): Generator<number> {
    yield 1;
    yield 2;
    yield 3;
}

function* rangeGenerator(start: number, end: number): Generator<number> {
    for (let i = start; i <= end; i++) {
        yield i;
    }
}

function* infiniteCounter(): Generator<number> {
    let n = 0;
    while (true) {
        yield n++;
    }
}

const gen = numberGenerator();
console.log(gen.next().value);
console.log(gen.next().value);"#;

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
        output.contains("numberGenerator") || output.contains("rangeGenerator"),
        "expected output to contain numberGenerator or rangeGenerator. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generator functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_async() {
    // Test async function declarations
    let source = r#"async function fetchData(url: string): Promise<string> {
    const response = await fetch(url);
    return response.text();
}

async function processItems(items: number[]): Promise<number[]> {
    const results: number[] = [];
    for (const item of items) {
        const processed = await Promise.resolve(item * 2);
        results.push(processed);
    }
    return results;
}

async function delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

fetchData("https://example.com").then(console.log);"#;

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
        output.contains("fetchData") || output.contains("processItems"),
        "expected output to contain fetchData or processItems. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for async functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_destructuring_params() {
    // Test functions with destructuring parameters
    let source = r#"function processPoint({ x, y }: { x: number; y: number }): number {
    return x + y;
}

function formatUser({ name, age = 0 }: { name: string; age?: number }): string {
    return name + " (" + age + ")";
}

function sumArray([first, second, ...rest]: number[]): number {
    return first + second + rest.reduce((a, b) => a + b, 0);
}

console.log(processPoint({ x: 10, y: 20 }));
console.log(formatUser({ name: "Alice" }));
console.log(sumArray([1, 2, 3, 4, 5]));"#;

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
        output.contains("processPoint") || output.contains("formatUser"),
        "expected output to contain processPoint or formatUser. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for destructuring params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_generic() {
    // Test generic function declarations
    let source = r#"function identity<T>(value: T): T {
    return value;
}

function map<T, U>(items: T[], fn: (item: T) => U): U[] {
    const results: U[] = [];
    for (const item of items) {
        results.push(fn(item));
    }
    return results;
}

function swap<T, U>(pair: [T, U]): [U, T] {
    return [pair[1], pair[0]];
}

console.log(identity<number>(42));
console.log(map<number, string>([1, 2, 3], (n) => String(n)));
console.log(swap<string, number>(["hello", 42]));"#;

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
        output.contains("identity") || output.contains("map"),
        "expected output to contain identity or map. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for generic functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_function_declaration_combined() {
    // Test combined function declaration patterns
    let source = r#"function createCalculator(initialValue: number = 0) {
    let value = initialValue;

    function add(n: number): void {
        value += n;
    }

    function subtract(n: number): void {
        value -= n;
    }

    function getValue(): number {
        return value;
    }

    async function asyncMultiply(n: number): Promise<number> {
        return value * n;
    }

    function* valueHistory(): Generator<number> {
        yield initialValue;
        yield value;
    }

    return {
        add,
        subtract,
        getValue,
        asyncMultiply,
        valueHistory
    };
}

function processData<T>(
    items: T[],
    { filter = (x: T) => true, transform = (x: T) => x }: {
        filter?: (item: T) => boolean;
        transform?: (item: T) => T;
    } = {}
): T[] {
    return items.filter(filter).map(transform);
}

function compose<A, B, C>(
    f: (a: A) => B,
    g: (b: B) => C
): (a: A) => C {
    return function(a: A): C {
        return g(f(a));
    };
}

const calc = createCalculator(10);
calc.add(5);
console.log(calc.getValue());

const numbers = processData([1, 2, 3, 4, 5], {
    filter: (n) => n > 2,
    transform: (n) => n * 2
});
console.log(numbers);"#;

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
        output.contains("createCalculator") || output.contains("processData"),
        "expected output to contain createCalculator or processData. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for combined function patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// =============================================================================
// Class Declaration ES5 Source Map Tests
// =============================================================================

#[test]
fn test_source_map_class_declaration_basic() {
    // Test basic class declaration with ES5 downleveling
    let source = r#"class Animal {
    name: string;

    constructor(name: string) {
        this.name = name;
    }
}

const dog = new Animal("Rex");
console.log(dog.name);"#;

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
        output.contains("Animal"),
        "expected output to contain Animal. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic class"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_with_methods() {
    // Test class with instance methods
    let source = r#"class Calculator {
    private value: number;

    constructor(initial: number = 0) {
        this.value = initial;
    }

    add(n: number): this {
        this.value += n;
        return this;
    }

    subtract(n: number): this {
        this.value -= n;
        return this;
    }

    multiply(n: number): this {
        this.value *= n;
        return this;
    }

    getResult(): number {
        return this.value;
    }
}

const calc = new Calculator(10);
console.log(calc.add(5).multiply(2).getResult());"#;

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
        output.contains("Calculator") || output.contains("add"),
        "expected output to contain Calculator or add. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class methods"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_static_members() {
    // Test class with static methods and properties
    let source = r#"class Counter {
    static count: number = 0;

    static increment(): void {
        Counter.count++;
    }

    static decrement(): void {
        Counter.count--;
    }

    static getCount(): number {
        return Counter.count;
    }

    static reset(): void {
        Counter.count = 0;
    }
}

Counter.increment();
Counter.increment();
console.log(Counter.getCount());
Counter.reset();"#;

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
        output.contains("Counter") || output.contains("increment"),
        "expected output to contain Counter or increment. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for static members"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_getters_setters() {
    // Test class with getter and setter accessors
    let source = r#"class Person {
    private _firstName: string;
    private _lastName: string;
    private _age: number;

    constructor(firstName: string, lastName: string, age: number) {
        this._firstName = firstName;
        this._lastName = lastName;
        this._age = age;
    }

    get fullName(): string {
        return this._firstName + " " + this._lastName;
    }

    set fullName(value: string) {
        const parts = value.split(" ");
        this._firstName = parts[0] || "";
        this._lastName = parts[1] || "";
    }

    get age(): number {
        return this._age;
    }

    set age(value: number) {
        if (value >= 0) {
            this._age = value;
        }
    }
}

const person = new Person("John", "Doe", 30);
console.log(person.fullName);
person.fullName = "Jane Smith";
console.log(person.age);"#;

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
        output.contains("Person") || output.contains("fullName"),
        "expected output to contain Person or fullName. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for getters/setters"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

