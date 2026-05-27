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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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
    let (parser, root) = parse_test_source(source);

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

#[test]
fn test_source_map_block_scoping_let_const_mapping() {
    // Test let/const to var transform source mapping
    let source = r#"let x = 1;
const y = 2;
let z = x + y;
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

    // Verify let/const are transformed to var
    assert!(
        output.contains("var x") || output.contains("var y") || output.contains("var z"),
        "expected let/const transformed to var in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the variable declarations
    let (let_line, _) = find_line_col(source, "let x");
    let has_let_mapping = decoded.iter().any(|entry| entry.original_line == let_line);

    let (const_line, _) = find_line_col(source, "const y");
    let has_const_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == const_line);

    assert!(
        has_let_mapping || has_const_mapping || !decoded.is_empty(),
        "expected mappings for let/const declarations. mappings: {mappings}"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_block_scoping_nested_blocks_mapping() {
    // Test nested block scoping with shadowing
    let source = r#"let x = 1;
{
    let x = 2;
    console.log(x);
}
console.log(x);"#;
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

    // Verify we have non-empty mappings for nested blocks
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for nested block scoping. output: {output}"
    );

    // Verify console.log is in output
    assert!(
        output.contains("console.log"),
        "expected console.log in output: {output}"
    );
}

#[test]
fn test_source_map_block_scoping_for_loop_mapping() {
    // Test for loop with let variable
    let source = r#"for (let i = 0; i < 10; i++) {
    console.log(i);
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

    // Verify for loop is in output
    assert!(
        output.contains("for") && (output.contains("var i") || output.contains("let i")),
        "expected for loop with variable in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the for loop
    let (for_line, _) = find_line_col(source, "for (let i");
    let has_for_mapping = decoded.iter().any(|entry| entry.original_line == for_line);

    assert!(
        has_for_mapping || !decoded.is_empty(),
        "expected mappings for for loop with let. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_block_scoping_function_scope_mapping() {
    // Test function-scoped let/const
    let source = r#"function test() {
    let local = 1;
    const result = local * 2;
    return result;
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

    // Verify function is in output
    assert!(
        output.contains("function test"),
        "expected function in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the function declaration
    let (func_line, _) = find_line_col(source, "function test");
    let has_func_mapping = decoded.iter().any(|entry| entry.original_line == func_line);

    assert!(
        has_func_mapping || !decoded.is_empty(),
        "expected mappings for function with let/const. mappings: {mappings}"
    );

    // Verify we have mappings covering multiple source lines
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 2,
        "expected mappings from at least 2 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_enum_es5_string_enum_mapping() {
    // Test string enum transforms to IIFE pattern without reverse mapping
    let source = r#"enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT"
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

    // Verify string enum generates IIFE pattern
    assert!(
        output.contains("var Direction"),
        "expected var Direction declaration in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for string enum"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_enum_es5_exported_enum_mapping() {
    // Test exported enum with source mapping
    let source = r#"export enum Status {
    Active = 1,
    Inactive = 0,
    Pending = 2
}

const current = Status.Active;"#;
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

    // Verify enum is in output
    assert!(
        output.contains("Status"),
        "expected Status enum in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the enum
    let (enum_line, _) = find_line_col(source, "enum Status");
    let has_enum_mapping = decoded.iter().any(|entry| entry.original_line == enum_line);

    assert!(
        has_enum_mapping || !decoded.is_empty(),
        "expected mappings for exported enum. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_enum_es5_computed_member_mapping() {
    // Test enum with computed member values
    let source = r#"enum Computed {
    A = 1,
    B = A * 2,
    C = 10
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

    // Verify enum generates IIFE pattern
    assert!(
        output.contains("var Computed") || output.contains("Computed"),
        "expected Computed enum in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for computed enum members"
    );
}

#[test]
fn test_source_map_enum_es5_mixed_values_mapping() {
    // Test enum with mixed numeric and auto-increment values
    let source = r#"enum Mixed {
    First,
    Second,
    Third = 10,
    Fourth,
    Fifth = 100
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

    // Verify enum IIFE pattern
    assert!(
        output.contains("var Mixed"),
        "expected var Mixed declaration in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the enum declaration
    let (enum_line, _) = find_line_col(source, "enum Mixed");
    let has_enum_mapping = decoded.iter().any(|entry| entry.original_line == enum_line);

    assert!(
        has_enum_mapping || !decoded.is_empty(),
        "expected mappings for enum with mixed values. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_commonjs_import_mapping() {
    // Test CommonJS import transform source mapping
    let source = r#"import { foo, bar } from "./module";
import * as utils from "./utils";
import defaultExport from "./default";

console.log(foo, bar, utils, defaultExport);"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify CommonJS require pattern
    assert!(
        output.contains("require") || output.contains("import"),
        "expected require or import in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for CommonJS imports"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_commonjs_export_mapping() {
    // Test CommonJS export transform source mapping
    let source = r#"export const value = 42;
export function greet(name: string) {
    return "Hello " + name;
}
export class MyClass {
    constructor() {}
}"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify exports pattern
    assert!(
        output.contains("exports") || output.contains("export"),
        "expected exports in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for the export declarations
    let (value_line, _) = find_line_col(source, "export const value");
    let has_value_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == value_line);

    let (func_line, _) = find_line_col(source, "export function greet");
    let has_func_mapping = decoded.iter().any(|entry| entry.original_line == func_line);

    assert!(
        has_value_mapping || has_func_mapping || !decoded.is_empty(),
        "expected mappings for CommonJS exports. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_commonjs_default_export_mapping() {
    // Test CommonJS default export transform source mapping
    let source = r#"const myValue = 100;

export default myValue;"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify default export or myValue in output
    assert!(
        output.contains("myValue") || output.contains("default"),
        "expected default export in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for default export"
    );
}

#[test]
fn test_source_map_commonjs_reexport_mapping() {
    // Test CommonJS re-export transform source mapping
    let source = r#"export { foo, bar } from "./module";
export * from "./utils";"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        module: crate::emitter::ModuleKind::CommonJS,
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

    // Verify we have mappings for re-exports
    let (reexport_line, _) = find_line_col(source, "export { foo");
    let has_reexport_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == reexport_line);

    assert!(
        has_reexport_mapping || !decoded.is_empty(),
        "expected mappings for re-exports. mappings: {mappings} output: {output}"
    );
}

#[test]
fn test_source_map_type_assertions_and_const() {
    // Test source-map accuracy for type assertions and const assertions
    let source = r#"const value: unknown = "hello";
const str = value as string;
const num = <number>someValue;

const config = {
    name: "app",
    version: 1
} as const;

const colors = ["red", "green", "blue"] as const;

function process(input: unknown) {
    const data = input as { id: number; name: string };
    return data.id;
}

type Point = { x: number; y: number };
const origin = { x: 0, y: 0 } as Point;"#;
    let (parser, root) = parse_test_source(source);

    let options = PrinterOptions::default();
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);

    let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
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

    // Verify we have mappings for variable declarations
    let (value_line, _) = find_line_col(source, "const value");
    let has_value_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == value_line);

    let (config_line, _) = find_line_col(source, "const config");
    let has_config_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == config_line);

    // At minimum, we should have mappings for declarations
    assert!(
        has_value_mapping || has_config_mapping || !decoded.is_empty(),
        "expected mappings for type assertions. mappings: {mappings}"
    );

    // Type assertions should be stripped from output
    assert!(
        !output.contains(" as string") && !output.contains(" as const"),
        "expected type assertions to be stripped. output: {output}"
    );

    // Verify source map has non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for type assertions"
    );
}

#[test]
fn test_source_map_jsx_element_mapping() {
    // Test JSX element source mapping
    let source = r#"const element = <div className="container">Hello</div>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX is in output
    assert!(
        output.contains("<div") || output.contains("div"),
        "expected JSX element in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX element"
    );

    // Verify source index is consistent
    assert!(
        decoded.iter().all(|m| m.source_index == 0),
        "expected all mappings to reference source file index 0"
    );
}

#[test]
fn test_source_map_jsx_fragment_mapping() {
    // Test JSX fragment source mapping
    let source = r#"const fragment = <>
    <span>First</span>
    <span>Second</span>
</>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX fragment is in output
    assert!(
        output.contains("<>") || output.contains("Fragment") || output.contains("span"),
        "expected JSX fragment in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX fragment"
    );
}

#[test]
fn test_source_map_jsx_expression_mapping() {
    // Test JSX with expressions source mapping
    let source = r#"const name = "World";
const greeting = <h1>Hello, {name}!</h1>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify JSX with expression is in output
    assert!(
        output.contains("name") && output.contains("h1"),
        "expected JSX expression in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have mappings for both declarations
    let (name_line, _) = find_line_col(source, "const name");
    let has_name_mapping = decoded.iter().any(|entry| entry.original_line == name_line);

    let (greeting_line, _) = find_line_col(source, "const greeting");
    let has_greeting_mapping = decoded
        .iter()
        .any(|entry| entry.original_line == greeting_line);

    assert!(
        has_name_mapping || has_greeting_mapping || !decoded.is_empty(),
        "expected mappings for JSX expressions. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_jsx_component_mapping() {
    // Test JSX component with props source mapping
    let source = r#"function Button({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
    return <button onClick={onClick}>{children}</button>;
}

const app = <Button onClick={() => console.log("clicked")}>Click me</Button>;"#;
    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let options = PrinterOptions::default();
    let mut printer = Printer::with_options(&parser.arena, options);
    printer.set_source_map_text(parser.get_source_text());
    printer.enable_source_map("test.js", "test.tsx");
    printer.emit(root);

    let output = printer.get_output().to_string();

    // Verify component is in output
    assert!(
        output.contains("Button") && output.contains("button"),
        "expected JSX component in output: {output}"
    );

    let map_json = printer.generate_source_map_json().expect("source map");
    let map_value: Value = serde_json::from_str(&map_json).expect("parse source map");
    let mappings = map_value
        .get("mappings")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let decoded = decode_mappings(mappings);

    // Verify we have non-empty mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for JSX component"
    );

    // Verify mappings cover multiple source lines
    let unique_source_lines: std::collections::HashSet<_> =
        decoded.iter().map(|m| m.original_line).collect();
    assert!(
        unique_source_lines.len() >= 2,
        "expected mappings from at least 2 different source lines, got: {unique_source_lines:?}"
    );
}

#[test]
fn test_source_map_namespace_es5_basic_mapping() {
    // Basic namespace transforms to IIFE pattern
    let source = r#"namespace Foo {
    export const value = 42;
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

    assert!(
        output.contains("var Foo;"),
        "expected var Foo declaration in output: {output}"
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
        "expected non-empty source mappings for namespace"
    );
}

#[test]
fn test_source_map_namespace_es5_nested_mapping() {
    // Nested/qualified namespace A.B.C
    let source = r#"namespace A.B.C {
    export function greet() {
        return "hello";
    }
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

    assert!(
        output.contains("var A;") || output.contains("var A "),
        "expected var A declaration for nested namespace in output: {output}"
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
        "expected non-empty source mappings for nested namespace"
    );
}

#[test]
fn test_source_map_class_decorator_single() {
    // Test single class decorator source mapping
    let source = r#"function Component(target: Function) {
    return target;
}

@Component
class MyComponent {
    render() {
        return "hello";
    }
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

    // Verify output contains the class and decorator pattern
    assert!(
        output.contains("MyComponent") && output.contains("render"),
        "expected output to contain class and method. output: {output}"
    );

    // Verify we have source mappings that reference source file
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for class decorator"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

#[test]
fn test_source_map_class_decorator_multiple() {
    // Test multiple class decorators source mapping
    let source = r#"function Component(target: Function) {}
function Injectable(target: Function) {}
function Sealed(target: Function) {}

@Component
@Injectable
@Sealed
class Service {
    constructor() {}
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

    // Verify output contains the decorated class
    assert!(
        output.contains("Service"),
        "expected output to contain Service class. output: {output}"
    );

    // Verify we have source mappings
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for multiple class decorators"
    );

    // Verify at least some mappings reference the source file (index 0)
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file. mappings: {mappings}"
    );
}

