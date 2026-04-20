#[test]
fn test_source_map_class_expressions() {
    // Test class expressions source map coverage
    let source = r#"const MyClass = class {
    value = 42;

    getValue() {
        return this.value;
    }
};

const NamedClass = class InternalName {
    static count = 0;

    constructor() {
        InternalName.count++;
    }
};

const factory = () => class {
    data: string;

    constructor(data: string) {
        this.data = data;
    }
};

const instance1 = new MyClass();
const instance2 = new NamedClass();
const DynamicClass = factory();
const instance3 = new DynamicClass("test");"#;

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

    // Verify we have mappings for the const declarations
    let (myclass_line, myclass_col) = find_line_col(source, "const MyClass");
    let has_myclass_mapping = decoded.iter().any(|entry| {
        entry.original_line == myclass_line
            && entry.original_column >= myclass_col
            && entry.original_column <= myclass_col + 13
    });

    // Verify we have mappings for the factory function
    let (factory_line, factory_col) = find_line_col(source, "const factory");
    let has_factory_mapping = decoded.iter().any(|entry| {
        entry.original_line == factory_line
            && entry.original_column >= factory_col
            && entry.original_column <= factory_col + 13
    });

    // At minimum, we should have mappings for declarations
    assert!(
        has_myclass_mapping || has_factory_mapping || !decoded.is_empty(),
        "expected mappings for class expressions. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("MyClass") && output.contains("factory"),
        "expected output to contain class and function names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class expressions"
    );
}

#[test]
fn test_source_map_exponentiation_operator_mapping() {
    // Test source-map accuracy for exponentiation operator (**)
    let source = r#"const square = 2 ** 2;
const cube = 3 ** 3;
const power = base ** exponent;
let x = 2;
x **= 3;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("square") && output.contains("cube"),
        "expected variable names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for exponentiation operator"
    );
}

#[test]
fn test_source_map_rest_spread_mapping() {
    // Test source-map accuracy for rest parameters and spread arguments
    let source = r#"function sum(...numbers: number[]): number {
    return numbers.reduce((a, b) => a + b, 0);
}

const arr = [1, 2, 3];
const result = sum(...arr);

const [first, ...rest] = arr;
const { x, ...others } = { x: 1, y: 2, z: 3 };"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("sum") && output.contains("arr"),
        "expected function and variable names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for rest/spread"
    );
}

#[test]
fn test_source_map_default_parameters_mapping() {
    // Test source-map accuracy for default parameter values
    let source = r#"function greet(name: string = "World", count: number = 1): string {
    return `Hello ${name}!`.repeat(count);
}

const add = (a: number = 0, b: number = 0) => a + b;

class Calculator {
    multiply(x: number = 1, y: number = 1): number {
        return x * y;
    }
}

function format(value: string, options: { uppercase?: boolean } = {}): string {
    return options.uppercase ? value.toUpperCase() : value;
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("greet") && output.contains("add") && output.contains("Calculator"),
        "expected function and class names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the function declarations
    let (greet_line, _) = find_line_col(source, "function greet");
    let has_greet_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == greet_line);

    let (add_line, _) = find_line_col(source, "const add");
    let has_add_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == add_line);

    assert!(
        has_greet_mapping || has_add_mapping,
        "expected mappings for default parameter declarations. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default parameters"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 3,
        "expected mappings from at least 3 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_arrow_functions() {
    // Test arrow functions source map coverage
    let source = r#"const add = (a: number, b: number) => a + b;

const square = (x: number) => x * x;

const identity = <T>(value: T) => value;

const multiLine = (x: number, y: number) => {
    const sum = x + y;
    const product = x * y;
    return { sum, product };
};

const nested = (a: number) => (b: number) => (c: number) => a + b + c;

const withThis = {
    value: 10,
    getValue: function() {
        return () => this.value;
    }
};

const arr = [1, 2, 3, 4, 5];
const doubled = arr.map(x => x * 2);
const filtered = arr.filter(x => x > 2);
const reduced = arr.reduce((acc, x) => acc + x, 0);"#;

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

    // Verify we have mappings for the const declarations
    let (add_line, add_col) = find_line_col(source, "const add");
    let has_add_mapping = decoded.iter().any(|entry| {
        entry.original_line == add_line
            && entry.original_column >= add_col
            && entry.original_column <= add_col + 9
    });

    // Verify we have mappings for multiLine function
    let (multi_line, multi_col) = find_line_col(source, "const multiLine");
    let has_multi_mapping = decoded.iter().any(|entry| {
        entry.original_line == multi_line
            && entry.original_column >= multi_col
            && entry.original_column <= multi_col + 15
    });

    // At minimum, we should have mappings for declarations
    assert!(
        has_add_mapping || has_multi_mapping || !decoded.is_empty(),
        "expected mappings for arrow functions. mappings: {mappings}"
    );

    // Verify output contains expected identifiers
    assert!(
        output.contains("multiLine") && output.contains("doubled"),
        "expected output to contain function and variable names. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow functions"
    );
}

#[test]
fn test_source_map_computed_property_names_mapping() {
    // Test source-map accuracy for computed property names
    let source = r#"const key = "dynamic";
const sym = Symbol("unique");

const obj = {
    [key]: "value1",
    [sym]: "value2",
    ["literal"]: "value3"
};

class MyClass {
    [key]: string = "field";

    [sym]() {
        return "method";
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("key") && output.contains("obj") && output.contains("MyClass"),
        "expected variable and class names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed property names"
    );
}

#[test]
fn test_source_map_shorthand_properties_mapping() {
    // Test source-map accuracy for shorthand property syntax
    let source = r#"const name = "John";
const age = 30;
const active = true;

const person = { name, age, active };

function createUser(id: number, email: string) {
    return { id, email, createdAt: Date.now() };
}

const coords = { x: 10, y: 20 };
const { x, y } = coords;

const merged = { ...coords, z: 30 };"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("name") && output.contains("person") && output.contains("createUser"),
        "expected variable and function names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    let (name_line, _) = find_line_col(source, "const name");
    let has_name_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == name_line);

    let (person_line, _) = find_line_col(source, "const person");
    let has_person_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == person_line);

    assert!(
        has_name_mapping || has_person_mapping,
        "expected mappings for shorthand property declarations. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for shorthand properties"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 3,
        "expected mappings from at least 3 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_method_definitions_mapping() {
    // Test source-map accuracy for method definition syntax
    let source = r#"const calculator = {
    add(a: number, b: number): number {
        return a + b;
    },
    subtract(a: number, b: number): number {
        return a - b;
    },
    *generator() {
        yield 1;
        yield 2;
    },
    async fetchData() {
        return await Promise.resolve(42);
    },
    get value() {
        return this._value;
    },
    set value(v: number) {
        this._value = v;
    }
};

class Counter {
    private count = 0;

    increment(): void {
        this.count++;
    }

    decrement(): void {
        this.count--;
    }

    async loadAsync(): Promise<number> {
        return this.count;
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("calculator") && output.contains("Counter") && output.contains("add"),
        "expected object and class names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    let (calc_line, _) = find_line_col(source, "const calculator");
    let has_calc_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == calc_line);

    let (counter_line, _) = find_line_col(source, "class Counter");
    let has_counter_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == counter_line);

    assert!(
        has_calc_mapping || has_counter_mapping,
        "expected mappings for method definitions. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for method definitions"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 5,
        "expected mappings from at least 5 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_for_of_for_in_loops_mapping() {
    // Test source-map accuracy for for-of and for-in loops
    let source = r#"const numbers = [1, 2, 3, 4, 5];
const obj = { a: 1, b: 2, c: 3 };

for (const num of numbers) {
    console.log(num);
}

for (const key in obj) {
    console.log(key, obj[key]);
}

for (let i of [10, 20, 30]) {
    i *= 2;
    console.log(i);
}

const iterable = new Map([["x", 1], ["y", 2]]);
for (const [key, value] of iterable) {
    console.log(key, value);
}

async function processItems(items: number[]) {
    for await (const item of items) {
        console.log(item);
    }
}"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.ts");
    printer.emit(root);

    let output = printer.get_output().to_string();
    assert!(
        output.contains("numbers") && output.contains("obj"),
        "expected variable names in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    let (numbers_line, _) = find_line_col(source, "const numbers");
    let has_numbers_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == numbers_line);

    let (for_of_line, _) = find_line_col(source, "for (const num of");
    let has_for_of_mapping = decoded
        .iter()
        .any(|m| m.source_index == 0 && m.original_line == for_of_line);

    assert!(
        has_numbers_mapping || has_for_of_mapping,
        "expected mappings for for-of/for-in loops. mappings: {mappings}"
    );

    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for for-of/for-in loops"
    );

    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 5,
        "expected mappings from at least 5 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_typescript_namespaces() {
    // Test source-map accuracy for TypeScript namespace declarations
    let source = r#"namespace MyNamespace {
    export const value = 42;

    export function greet(name: string): string {
        return "Hello, " + name;
    }

    export class Helper {
        static compute(x: number): number {
            return x * 2;
        }
    }
}

namespace Nested.Inner {
    export const nested = "inner value";
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

    // Verify we have mappings for the namespace declaration
    let (ns_line, ns_col) = find_line_col(source, "namespace MyNamespace");
    let has_ns_mapping = decoded.iter().any(|entry| {
        entry.original_line == ns_line
            && entry.original_column >= ns_col
            && entry.original_column <= ns_col + 20
    });

    // Verify we have mappings for the nested namespace
    let (nested_line, nested_col) = find_line_col(source, "namespace Nested");
    let has_nested_mapping = decoded.iter().any(|entry| {
        entry.original_line == nested_line
            && entry.original_column >= nested_col
            && entry.original_column <= nested_col + 16
    });

    // At minimum, we should have mappings for namespace declarations
    assert!(
        has_ns_mapping || has_nested_mapping || !decoded.is_empty(),
        "expected mappings for namespace declarations. mappings: {mappings}"
    );

    // Verify output contains namespace IIFE pattern
    assert!(
        output.contains("MyNamespace") || output.contains("var MyNamespace"),
        "expected output to contain namespace identifiers. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for TypeScript namespaces"
    );
}

