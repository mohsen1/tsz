#[test]
fn test_source_map_spread_rest_es5_function_call_spread() {
    let source = r#"function sum(a: number, b: number, c: number): number {
    return a + b + c;
}

const args = [1, 2, 3] as const;
const result = sum(...args);
console.log(result);

function log(...items: any[]): void {
    console.log(...items);
}

const messages = ["hello", "world"];
log(...messages);

Math.max(...[1, 5, 3, 9, 2]);"#;

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
        output.contains("sum"),
        "expected sum function in output. output: {output}"
    );
    assert!(
        output.contains("log"),
        "expected log function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for function call spread"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_array_with_elements() {
    let source = r#"const middle = [3, 4, 5];
const full = [1, 2, ...middle, 6, 7];
console.log(full);

const prefix = [0];
const suffix = [9, 10];
const range = [...prefix, 1, 2, ...middle, 8, ...suffix];
console.log(range);

const items = ["b", "c"];
const alphabet = ["a", ...items, "d", "e"];
console.log(alphabet);"#;

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
        output.contains("middle"),
        "expected middle in output. output: {output}"
    );
    assert!(
        output.contains("full"),
        "expected full in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array spread with elements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_object_with_properties() {
    let source = r#"const base = { x: 1, y: 2 };
const extended = { z: 3, ...base, w: 4 };
console.log(extended);

const config = { debug: false };
const options = { verbose: true, ...config, timeout: 5000 };
console.log(options);

const user = { name: "Bob" };
const profile = { id: 1, ...user, role: "admin", ...{ active: true } };
console.log(profile);"#;

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
        output.contains("base"),
        "expected base in output. output: {output}"
    );
    assert!(
        output.contains("extended"),
        "expected extended in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object spread with properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_rest_parameters() {
    let source = r#"function collect(...items: number[]): number[] {
    return items;
}

function processAll(first: string, ...rest: string[]): string {
    return first + ": " + rest.join(", ");
}

const formatItems = (prefix: string, ...values: any[]) => {
    return prefix + values.map(v => String(v)).join("");
};

class Handler {
    handle(action: string, ...args: any[]): void {
        console.log(action, args);
    }
}

console.log(collect(1, 2, 3, 4, 5));
console.log(processAll("Items", "a", "b", "c"));
console.log(formatItems("Values: ", 1, 2, 3));"#;

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
        output.contains("collect"),
        "expected collect function in output. output: {output}"
    );
    assert!(
        output.contains("processAll"),
        "expected processAll function in output. output: {output}"
    );
    assert!(
        output.contains("Handler"),
        "expected Handler class in output. output: {output}"
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
fn test_source_map_spread_rest_es5_array_rest_elements() {
    let source = r#"const [first, ...remaining] = [1, 2, 3, 4, 5];
console.log(first, remaining);

const [head, second, ...tail] = [10, 20, 30, 40, 50];
console.log(head, second, tail);

function processArray([a, b, ...rest]: number[]): void {
    console.log(a, b, rest);
}

const numbers = [100, 200, 300, 400];
const [x, ...others] = numbers;
console.log(x, others);

processArray([1, 2, 3, 4, 5]);"#;

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
        output.contains("first"),
        "expected first in output. output: {output}"
    );
    assert!(
        output.contains("remaining"),
        "expected remaining in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for array rest elements"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_object_rest_properties() {
    let source = r#"const { name, ...rest } = { name: "Alice", age: 30, city: "NYC" };
console.log(name, rest);

const { id, type, ...metadata } = { id: 1, type: "user", active: true, role: "admin" };
console.log(id, type, metadata);

function extractUser({ username, ...props }: { username: string; [key: string]: any }): void {
    console.log(username, props);
}

const config = { host: "localhost", port: 8080, debug: true };
const { host, ...serverConfig } = config;
console.log(host, serverConfig);

extractUser({ username: "bob", email: "bob@example.com", verified: true });"#;

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
        output.contains("name"),
        "expected name in output. output: {output}"
    );
    assert!(
        output.contains("rest"),
        "expected rest in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for object rest properties"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_nested_patterns() {
    let source = r#"const data = {
    users: [
        { id: 1, name: "Alice", scores: [90, 85, 92] },
        { id: 2, name: "Bob", scores: [88, 91, 87] }
    ],
    metadata: { count: 2, page: 1 }
};

const { users: [first, ...otherUsers], ...restData } = data;
console.log(first, otherUsers, restData);

const nested = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
const [[a, ...row1Rest], ...otherRows] = nested;
console.log(a, row1Rest, otherRows);

function process({ items: [head, ...tail], ...options }: { items: number[]; [key: string]: any }) {
    console.log(head, tail, options);
}

process({ items: [1, 2, 3], debug: true, verbose: false });"#;

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
        output.contains("data"),
        "expected data in output. output: {output}"
    );
    assert!(
        output.contains("nested"),
        "expected nested in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested spread/rest patterns"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_spread_rest_es5_comprehensive() {
    let source = r#"// Comprehensive spread/rest patterns for ES5 transform testing

// Array spread
const arr1 = [1, 2, 3];
const arr2 = [4, 5, 6];
const combined = [...arr1, ...arr2];
const withElements = [0, ...arr1, 10, ...arr2, 20];

// Object spread
const obj1 = { a: 1, b: 2 };
const obj2 = { c: 3, d: 4 };
const merged = { ...obj1, ...obj2 };
const withProps = { prefix: "start", ...obj1, middle: true, ...obj2, suffix: "end" };

// Function call spread
function sum(...nums: number[]): number {
    return nums.reduce((a, b) => a + b, 0);
}
const numbers = [1, 2, 3, 4, 5];
const total = sum(...numbers);

// Rest parameters with different positions
function processArgs(first: string, second: number, ...rest: any[]): void {
    console.log(first, second, rest);
}

// Array rest elements
const [head, ...tail] = [1, 2, 3, 4, 5];
const [a, b, ...remaining] = numbers;

// Object rest properties
const { name, age, ...metadata } = { name: "Alice", age: 30, city: "NYC", country: "USA" };

// Nested patterns
const data = {
    users: [{ id: 1, ...obj1 }, { id: 2, ...obj2 }],
    settings: { ...merged, extra: true }
};
const { users: [firstUser, ...otherUsers], ...restData } = data;

// Class with spread/rest
class DataCollector {
    private items: any[];

    constructor(...initialItems: any[]) {
        this.items = [...initialItems];
    }

    add(...newItems: any[]): void {
        this.items = [...this.items, ...newItems];
    }

    getAll(): any[] {
        return [...this.items];
    }

    extract(): { first: any; rest: any[] } {
        const [first, ...rest] = this.items;
        return { first, rest };
    }
}

// Arrow functions with rest
const collectRest = (...items: any[]) => [...items];
const processRest = (first: any, ...rest: any[]) => ({ first, rest });

// Spread in new expression
class Point {
    constructor(public x: number, public y: number, public z?: number) {}
}
const coords = [10, 20, 30] as const;
const point = new Point(...coords);

// Usage
console.log(combined, withElements);
console.log(merged, withProps);
console.log(total);
processArgs("hello", 42, true, "extra", 123);
console.log(head, tail, a, b, remaining);
console.log(name, age, metadata);
console.log(firstUser, otherUsers, restData);

const collector = new DataCollector(1, 2, 3);
collector.add(4, 5);
console.log(collector.getAll());
console.log(collector.extract());

console.log(collectRest(1, 2, 3));
console.log(processRest("first", "a", "b", "c"));
console.log(point);"#;

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
        output.contains("sum"),
        "expected sum function in output. output: {output}"
    );
    assert!(
        output.contains("processArgs"),
        "expected processArgs function in output. output: {output}"
    );
    assert!(
        output.contains("DataCollector"),
        "expected DataCollector class in output. output: {output}"
    );
    assert!(
        output.contains("Point"),
        "expected Point class in output. output: {output}"
    );
    assert!(
        output.contains("collectRest"),
        "expected collectRest function in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive spread/rest"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

// ============================================================================
// ES5 Class Expression Transform Source Map Tests
// ============================================================================

#[test]
fn test_source_map_class_expr_es5_anonymous() {
    let source = r#"const MyClass = class {
    constructor(public value: number) {}

    getValue(): number {
        return this.value;
    }
};

const instance = new MyClass(42);
console.log(instance.getValue());

const factory = function() {
    return class {
        name: string = "default";
        greet() { return "Hello, " + this.name; }
    };
};

const Created = factory();
const obj = new Created();
console.log(obj.greet());"#;

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
        output.contains("MyClass"),
        "expected MyClass in output. output: {output}"
    );
    assert!(
        output.contains("getValue"),
        "expected getValue in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for anonymous class expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_class_expr_es5_named() {
    let source = r##"const MyClass = class NamedClass {
    static className = "NamedClass";

    constructor(public id: number) {}

    describe(): string {
        return NamedClass.className + "#" + this.id;
    }
};

const instance = new MyClass(1);
console.log(instance.describe());

const Container = class InnerClass {
    static create() {
        return new InnerClass();
    }

    value = 100;
};

const created = Container.create();
console.log(created.value);"##;

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
        output.contains("MyClass"),
        "expected MyClass in output. output: {output}"
    );
    assert!(
        output.contains("describe"),
        "expected describe method in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for named class expression"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

