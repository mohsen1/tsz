#[test]
fn test_source_map_arrow_es5_callbacks() {
    let source = r#"const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

const doubled = numbers.map(n => n * 2);
const evens = numbers.filter(n => n % 2 === 0);
const sum = numbers.reduce((acc, n) => acc + n, 0);
const found = numbers.find(n => n > 5);
const allPositive = numbers.every(n => n > 0);
const hasNegative = numbers.some(n => n < 0);

const sorted = [...numbers].sort((a, b) => b - a);

const users = [
    { name: "Alice", age: 30 },
    { name: "Bob", age: 25 },
    { name: "Charlie", age: 35 }
];

const names = users.map(u => u.name);
const adults = users.filter(u => u.age >= 18);
const totalAge = users.reduce((sum, u) => sum + u.age, 0);
const youngest = users.reduce((min, u) => u.age < min.age ? u : min);

setTimeout(() => console.log("delayed"), 0);
Promise.resolve(42).then(n => n * 2).then(n => console.log(n));

console.log(doubled, evens, sum, found);
console.log(names, adults, totalAge, youngest);"#;

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
        output.contains("doubled"),
        "expected doubled in output. output: {output}"
    );
    assert!(
        output.contains("users"),
        "expected users in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow callbacks"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_comprehensive() {
    let source = r#"// Comprehensive arrow function patterns for ES5 transform testing

// Expression body
const add = (a: number, b: number) => a + b;
const identity = <T>(x: T) => x;

// Block body
const process = (data: number[]) => {
    const result = data.map(x => x * 2);
    return result.filter(x => x > 5);
};

// Default parameters
const greet = (name: string = "World") => "Hello, " + name;
const configure = (host: string = "localhost", port: number = 8080) => ({ host, port });

// Rest parameters
const sum = (...nums: number[]) => nums.reduce((a, b) => a + b, 0);
const collect = (first: string, ...rest: string[]) => [first, ...rest];

// Destructuring parameters
const getPoint = ({ x, y }: { x: number; y: number }) => x + y;
const getFirst = ([a, b]: [number, number]) => a;

// This binding in class
class Timer {
    seconds = 0;

    tick = () => {
        this.seconds++;
        return this.seconds;
    };

    reset = () => {
        this.seconds = 0;
    };
}

// Higher-order functions
const createMultiplier = (factor: number) => (x: number) => x * factor;
const compose = <A, B, C>(f: (b: B) => C, g: (a: A) => B) => (x: A) => f(g(x));

// Callbacks
const numbers = [1, 2, 3, 4, 5];
const doubled = numbers.map(n => n * 2);
const evens = numbers.filter(n => n % 2 === 0);
const total = numbers.reduce((acc, n) => acc + n, 0);

// Async arrows
const fetchData = async (url: string) => {
    const response = await fetch(url);
    return response.json();
};

// Arrow IIFE
const result = ((x: number) => x * x)(5);

// Nested arrows
const outer = (a: number) => (b: number) => (c: number) => a + b + c;

// Arrow returning object
const createUser = (name: string, age: number) => ({ name, age, active: true });

// Generic arrow
const mapArray = <T, U>(arr: T[], fn: (x: T) => U) => arr.map(fn);

// Usage
console.log(add(1, 2));
console.log(process([1, 2, 3, 4, 5, 6]));
console.log(greet());
console.log(sum(1, 2, 3, 4, 5));
console.log(getPoint({ x: 10, y: 20 }));

const timer = new Timer();
console.log(timer.tick());
timer.reset();

const triple = createMultiplier(3);
console.log(triple(4));

console.log(doubled, evens, total);
console.log(result);
console.log(outer(1)(2)(3));
console.log(createUser("Alice", 30));
console.log(mapArray([1, 2, 3], x => x * 2));"#;

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
        output.contains("add"),
        "expected add in output. output: {output}"
    );
    assert!(
        output.contains("process"),
        "expected process in output. output: {output}"
    );
    assert!(
        output.contains("Timer"),
        "expected Timer class in output. output: {output}"
    );
    assert!(
        output.contains("createMultiplier"),
        "expected createMultiplier in output. output: {output}"
    );
    assert!(
        output.contains("fetchData"),
        "expected fetchData in output. output: {output}"
    );
    assert!(
        output.contains("mapArray"),
        "expected mapArray in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for comprehensive arrow functions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_basic() {
    let source = r#"const greeting = `Hello, World!`;
const simple = `Just a string`;
const empty = ``;
const withNewline = `Line 1
Line 2`;
console.log(greeting, simple, empty, withNewline);"#;

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
        output.contains("greeting"),
        "expected greeting in output. output: {output}"
    );
    assert!(
        output.contains("Hello"),
        "expected Hello in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for basic template literals"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_template_literal_es5_expression() {
    let source = r#"const name = "Alice";
const age = 30;
const message = `Hello, ${name}!`;
const info = `${name} is ${age} years old`;
const calc = `Result: ${2 + 3 * 4}`;
const nested = `Outer ${`Inner ${name}`}`;
console.log(message, info, calc, nested);"#;

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
        output.contains("name"),
        "expected name in output. output: {output}"
    );
    assert!(
        output.contains("message"),
        "expected message in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for template expressions"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}
