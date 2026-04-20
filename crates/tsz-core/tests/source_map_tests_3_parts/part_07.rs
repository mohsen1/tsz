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

