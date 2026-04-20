#[test]
fn test_source_map_arrow_es5_this_binding() {
    let source = r#"class Counter {
    count = 0;

    increment = () => {
        this.count++;
        return this.count;
    };

    decrement = () => {
        this.count--;
        return this.count;
    };

    addMultiple = (n: number) => {
        for (let i = 0; i < n; i++) {
            this.increment();
        }
        return this.count;
    };
}

class EventHandler {
    name = "Handler";

    handleClick = () => {
        console.log(this.name + " clicked");
    };

    handleHover = () => {
        console.log(this.name + " hovered");
    };
}

const counter = new Counter();
console.log(counter.increment());
console.log(counter.addMultiple(5));

const handler = new EventHandler();
handler.handleClick();
handler.handleHover();"#;

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
        output.contains("Counter"),
        "expected Counter class in output. output: {output}"
    );
    assert!(
        output.contains("EventHandler"),
        "expected EventHandler class in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow this binding"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_rest_params() {
    let source = r#"const sum = (...numbers: number[]) => numbers.reduce((a, b) => a + b, 0);

const concat = (...strings: string[]) => strings.join("");

const first = <T>(first: T, ...rest: T[]) => first;

const last = <T>(...items: T[]) => items[items.length - 1];

const collect = (prefix: string, ...values: any[]) => {
    return prefix + ": " + values.join(", ");
};

const spread = (...args: number[]) => {
    const [head, ...tail] = args;
    return { head, tail };
};

console.log(sum(1, 2, 3, 4, 5));
console.log(concat("a", "b", "c"));
console.log(first(1, 2, 3));
console.log(last(1, 2, 3, 4));
console.log(collect("Items", 1, 2, 3));
console.log(spread(10, 20, 30, 40));"#;

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
        "expected sum in output. output: {output}"
    );
    assert!(
        output.contains("concat"),
        "expected concat in output. output: {output}"
    );
    assert!(
        output.contains("collect"),
        "expected collect in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow rest params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_default_params() {
    let source = r#"const greet = (name: string = "World") => "Hello, " + name;

const power = (base: number, exp: number = 2) => Math.pow(base, exp);

const configure = (host: string = "localhost", port: number = 8080) => {
    return { host, port };
};

const format = (value: number, prefix: string = "$", suffix: string = "") => {
    return prefix + value.toFixed(2) + suffix;
};

const createUser = (name: string, age: number = 0, active: boolean = true) => ({
    name,
    age,
    active
});

console.log(greet());
console.log(greet("Alice"));
console.log(power(2));
console.log(power(2, 10));
console.log(configure());
console.log(format(123.456));
console.log(createUser("Bob"));"#;

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
        "expected greet in output. output: {output}"
    );
    assert!(
        output.contains("power"),
        "expected power in output. output: {output}"
    );
    assert!(
        output.contains("configure"),
        "expected configure in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow default params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_destructuring_params() {
    let source = r#"const getFullName = ({ first, last }: { first: string; last: string }) =>
    first + " " + last;

const getCoords = ([x, y]: [number, number]) => ({ x, y });

const processUser = ({ name, age = 0 }: { name: string; age?: number }) => {
    return name + " is " + age + " years old";
};

const extractValues = ({ a, b, ...rest }: { a: number; b: number; [key: string]: number }) => {
    return { sum: a + b, rest };
};

const handleEvent = ({ type, target: { id } }: { type: string; target: { id: string } }) => {
    console.log(type + " on " + id);
};

console.log(getFullName({ first: "John", last: "Doe" }));
console.log(getCoords([10, 20]));
console.log(processUser({ name: "Alice" }));
console.log(extractValues({ a: 1, b: 2, c: 3, d: 4 }));"#;

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
        output.contains("getFullName"),
        "expected getFullName in output. output: {output}"
    );
    assert!(
        output.contains("processUser"),
        "expected processUser in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow destructuring params"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_class_property() {
    let source = r#"class Calculator {
    add = (a: number, b: number) => a + b;
    subtract = (a: number, b: number) => a - b;
    multiply = (a: number, b: number) => a * b;
    divide = (a: number, b: number) => b !== 0 ? a / b : 0;
}

class Formatter {
    prefix: string;
    suffix: string;

    constructor(prefix: string = "", suffix: string = "") {
        this.prefix = prefix;
        this.suffix = suffix;
    }

    format = (value: any) => this.prefix + String(value) + this.suffix;
    formatNumber = (n: number) => this.format(n.toFixed(2));
}

class Logger {
    static log = (message: string) => console.log("[LOG] " + message);
    static warn = (message: string) => console.warn("[WARN] " + message);
    static error = (message: string) => console.error("[ERROR] " + message);
}

const calc = new Calculator();
console.log(calc.add(1, 2));
console.log(calc.multiply(3, 4));

const fmt = new Formatter("$", " USD");
console.log(fmt.formatNumber(123.456));

Logger.log("Test message");"#;

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
        output.contains("Calculator"),
        "expected Calculator in output. output: {output}"
    );
    assert!(
        output.contains("Formatter"),
        "expected Formatter in output. output: {output}"
    );
    assert!(
        output.contains("Logger"),
        "expected Logger in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for arrow class property"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

#[test]
fn test_source_map_arrow_es5_higher_order() {
    let source = r#"const createAdder = (n: number) => (x: number) => x + n;

const createMultiplier = (factor: number) => (x: number) => x * factor;

const compose = <A, B, C>(f: (b: B) => C, g: (a: A) => B) => (x: A) => f(g(x));

const pipe = <T>(...fns: ((x: T) => T)[]) => (x: T) => fns.reduce((v, f) => f(v), x);

const curry = (fn: (a: number, b: number) => number) => (a: number) => (b: number) => fn(a, b);

const memoize = <T, R>(fn: (arg: T) => R) => {
    const cache = new Map<T, R>();
    return (arg: T) => {
        if (cache.has(arg)) return cache.get(arg)!;
        const result = fn(arg);
        cache.set(arg, result);
        return result;
    };
};

const add5 = createAdder(5);
const double = createMultiplier(2);
const addThenDouble = compose(double, add5);

console.log(add5(10));
console.log(double(7));
console.log(addThenDouble(3));

const increment = (x: number) => x + 1;
const triple = (x: number) => x * 3;
const pipeline = pipe(increment, double, triple);
console.log(pipeline(2));"#;

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
        output.contains("createAdder"),
        "expected createAdder in output. output: {output}"
    );
    assert!(
        output.contains("compose"),
        "expected compose in output. output: {output}"
    );
    assert!(
        output.contains("memoize"),
        "expected memoize in output. output: {output}"
    );
    assert!(
        !decoded.is_empty(),
        "expected non-empty source mappings for higher order arrows"
    );
    let has_source_mapping = decoded.iter().any(|entry| entry.source_index == 0);
    assert!(
        has_source_mapping,
        "expected mappings to reference source file"
    );
}

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

// =============================================================================
// ES5 Template Literal Transform Source Map Tests
// =============================================================================

#[test]
fn test_source_map_template_literal_es5_basic() {
    let source = r#"const greeting = `Hello, World!`;
const simple = `Just a string`;
const empty = ``;
const withNewline = `Line 1
Line 2`;
console.log(greeting, simple, empty, withNewline);"#;

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

