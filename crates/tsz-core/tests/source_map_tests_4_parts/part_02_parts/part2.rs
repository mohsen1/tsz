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
